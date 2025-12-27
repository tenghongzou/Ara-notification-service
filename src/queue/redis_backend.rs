//! Redis-based message queue backend using Redis Streams.
//!
//! This module provides a persistent implementation of the `MessageQueueBackend` trait
//! using Redis Streams for storage. Messages are persisted and survive service restarts.

use std::sync::Arc;

use async_trait::async_trait;

use crate::notification::NotificationEvent;
use crate::redis::pool::{PoolError, RedisPool, RedisPoolExt};

use super::backend::{
    DrainResult, MessageQueueBackend, QueueBackendError, QueueBackendStats, StoredMessage,
};
use super::QueueConfig;

/// Redis-based message queue backend.
///
/// Uses Redis Streams for persistent message storage.
/// Each user has a dedicated stream: `{prefix}:{tenant_id}:{user_id}`.
pub struct RedisQueueBackend {
    /// Redis connection pool
    pool: Arc<RedisPool>,

    /// Configuration
    config: QueueConfig,

    /// Key prefix for Redis keys
    prefix: String,

    /// Default tenant ID
    tenant_id: String,
}

impl RedisQueueBackend {
    /// Create a new Redis queue backend.
    pub fn new(
        config: QueueConfig,
        pool: Arc<RedisPool>,
        prefix: String,
    ) -> Self {
        Self {
            pool,
            config,
            prefix,
            tenant_id: "default".to_string(),
        }
    }

    /// Create a new Redis queue backend with a specific tenant ID.
    pub fn with_tenant(
        config: QueueConfig,
        pool: Arc<RedisPool>,
        prefix: String,
        tenant_id: String,
    ) -> Self {
        Self {
            pool,
            config,
            prefix,
            tenant_id,
        }
    }

    /// Generate the Redis key for a user's queue.
    fn queue_key(&self, user_id: &str) -> String {
        format!("{}:{}:{}", self.prefix, self.tenant_id, user_id)
    }

    /// Convert pool error to queue backend error.
    fn map_error(err: PoolError) -> QueueBackendError {
        match err {
            PoolError::Redis(e) => QueueBackendError::Redis(e),
            PoolError::CircuitOpen => {
                QueueBackendError::Unavailable("Circuit breaker is open".to_string())
            }
            PoolError::ConnectionUnavailable(msg) => QueueBackendError::Unavailable(msg),
        }
    }
}

#[async_trait]
impl MessageQueueBackend for RedisQueueBackend {
    fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    fn message_ttl_seconds(&self) -> u64 {
        self.config.message_ttl_seconds
    }

    async fn enqueue(&self, user_id: &str, event: NotificationEvent) -> Result<(), QueueBackendError> {
        if !self.config.enabled {
            return Err(QueueBackendError::Disabled);
        }

        let message = StoredMessage::new(event);
        let key = self.queue_key(user_id);

        // Serialize the message
        let msg_json = serde_json::to_string(&message)?;

        // Add to stream with MAXLEN trimming
        let fields = [
            ("data", msg_json.as_str()),
        ];

        self.pool
            .xadd_maxlen(&key, self.config.max_queue_size_per_user, &fields)
            .await
            .map_err(Self::map_error)?;

        tracing::debug!(
            user_id = %user_id,
            message_id = %message.id,
            key = %key,
            "Message enqueued to Redis stream"
        );

        Ok(())
    }

    async fn drain(&self, user_id: &str) -> Result<DrainResult, QueueBackendError> {
        if !self.config.enabled {
            return Ok(DrainResult::default());
        }

        let key = self.queue_key(user_id);

        // Read all entries from the stream
        let entries = self.pool.xrange_all(&key).await.map_err(Self::map_error)?;

        if entries.is_empty() {
            return Ok(DrainResult::default());
        }

        // Parse messages and filter expired
        let ttl = self.config.message_ttl_seconds;
        let mut messages = Vec::new();
        let mut expired = 0;

        for (stream_id, fields) in entries {
            // Find the data field
            let data = fields.iter().find(|(k, _)| k == "data").map(|(_, v)| v);

            if let Some(json) = data {
                match serde_json::from_str::<StoredMessage>(json) {
                    Ok(mut msg) => {
                        msg.stream_id = Some(stream_id);
                        if msg.is_expired(ttl) {
                            expired += 1;
                        } else {
                            messages.push(msg);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            key = %key,
                            "Failed to deserialize queued message"
                        );
                    }
                }
            }
        }

        // Delete the stream after draining
        if !messages.is_empty() || expired > 0 {
            self.pool.del(&key).await.map_err(Self::map_error)?;
        }

        tracing::info!(
            user_id = %user_id,
            message_count = messages.len(),
            expired = expired,
            "Drained message queue from Redis"
        );

        Ok(DrainResult { messages, expired })
    }

    async fn peek(&self, user_id: &str, limit: usize) -> Result<Vec<StoredMessage>, QueueBackendError> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let key = self.queue_key(user_id);

        // Read entries from the stream
        let entries = self.pool.xrange_all(&key).await.map_err(Self::map_error)?;

        let mut messages = Vec::new();

        for (stream_id, fields) in entries.into_iter().take(limit) {
            let data = fields.iter().find(|(k, _)| k == "data").map(|(_, v)| v);

            if let Some(json) = data {
                if let Ok(mut msg) = serde_json::from_str::<StoredMessage>(json) {
                    msg.stream_id = Some(stream_id);
                    messages.push(msg);
                }
            }
        }

        Ok(messages)
    }

    async fn queue_size(&self, user_id: &str) -> Result<usize, QueueBackendError> {
        if !self.config.enabled {
            return Ok(0);
        }

        let key = self.queue_key(user_id);

        // Read all entries and count them
        let entries = self.pool.xrange_all(&key).await.map_err(Self::map_error)?;

        Ok(entries.len())
    }

    async fn cleanup_expired(&self) -> Result<usize, QueueBackendError> {
        // For Redis backend, we rely on MAXLEN trimming during XADD
        // and message expiry during drain.
        // A full cleanup would require scanning all keys, which is expensive.
        // This is a trade-off for simplicity.
        Ok(0)
    }

    async fn clear_user_queue(&self, user_id: &str) -> Result<usize, QueueBackendError> {
        let key = self.queue_key(user_id);

        // Get count before deleting
        let count = self.queue_size(user_id).await?;

        // Delete the stream
        self.pool.del(&key).await.map_err(Self::map_error)?;

        Ok(count)
    }

    async fn stats(&self) -> QueueBackendStats {
        // For Redis backend, getting accurate stats would require
        // scanning all keys, which is expensive. Return basic info.
        QueueBackendStats {
            backend_type: "redis".to_string(),
            enabled: self.config.enabled,
            total_messages: 0, // Would require SCAN
            users_with_queue: 0, // Would require SCAN
            max_queue_size: 0,
            max_queue_size_config: self.config.max_queue_size_per_user,
            message_ttl_seconds: self.config.message_ttl_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_key_generation() {
        let config = QueueConfig::default();
        let pool = create_mock_pool();
        let backend = RedisQueueBackend::new(config, pool, "ara:queue".to_string());

        assert_eq!(backend.queue_key("user-123"), "ara:queue:default:user-123");
    }

    #[test]
    fn test_queue_key_with_tenant() {
        let config = QueueConfig::default();
        let pool = create_mock_pool();
        let backend = RedisQueueBackend::with_tenant(
            config,
            pool,
            "ara:queue".to_string(),
            "tenant-abc".to_string(),
        );

        assert_eq!(backend.queue_key("user-123"), "ara:queue:tenant-abc:user-123");
    }

    fn create_mock_pool() -> Arc<RedisPool> {
        // Create a mock pool for testing key generation
        // Actual Redis tests would use #[ignore] and require a real Redis
        use crate::config::RedisConfig;
        use crate::redis::{CircuitBreaker, RedisHealth};

        let config = RedisConfig {
            url: "redis://localhost:6379".to_string(),
            channels: vec![],
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_success_threshold: 2,
            circuit_breaker_reset_timeout_seconds: 30,
            backoff_initial_delay_ms: 100,
            backoff_max_delay_ms: 30000,
        };

        let cb = Arc::new(CircuitBreaker::new());
        let health = Arc::new(RedisHealth::new());

        Arc::new(RedisPool::new(config, cb, health).unwrap())
    }
}

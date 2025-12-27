//! Redis-based ACK tracking backend using Hash and Sorted Set.
//!
//! This module provides a persistent implementation of the `AckTrackerBackend` trait
//! using Redis Hash for pending ACK storage and Sorted Set for timeout tracking.
//! ACK tracking state survives service restarts.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::metrics::{ACK_EXPIRED_TOTAL, ACK_LATENCY, ACK_RECEIVED_TOTAL, ACK_TRACKED_TOTAL};
use crate::notification::ack::AckConfig;
use crate::redis::pool::{PoolError, RedisPool, RedisPoolExt};

use super::ack_backend::{AckBackendError, AckBackendStats, AckTrackerBackend, PendingAckInfo};

/// Redis-based ACK tracking backend.
///
/// Uses Redis Hash for storing pending ACK info and Sorted Set for timeout tracking.
///
/// Key structure:
/// - `{prefix}:{tenant_id}:pending:{notification_id}` - Pending ACK info (Hash)
/// - `{prefix}:{tenant_id}:timeout` - Timeout tracking (Sorted Set, score = expiry timestamp)
/// - `{prefix}:{tenant_id}:stats` - Statistics counters (Hash)
pub struct RedisAckBackend {
    /// Redis connection pool
    pool: Arc<RedisPool>,

    /// Configuration
    config: AckConfig,

    /// Key prefix for Redis keys
    prefix: String,

    /// Tenant ID for multi-tenant isolation
    tenant_id: String,
}

impl RedisAckBackend {
    /// Create a new Redis ACK backend.
    pub fn new(config: AckConfig, pool: Arc<RedisPool>, prefix: String) -> Self {
        Self {
            pool,
            config,
            prefix,
            tenant_id: "default".to_string(),
        }
    }

    /// Create a new Redis ACK backend with a specific tenant ID.
    pub fn with_tenant(
        config: AckConfig,
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

    /// Generate the Redis key for a pending ACK.
    fn pending_key(&self, notification_id: &Uuid) -> String {
        format!(
            "{}:{}:pending:{}",
            self.prefix, self.tenant_id, notification_id
        )
    }

    /// Generate the Redis key for the timeout sorted set.
    fn timeout_key(&self) -> String {
        format!("{}:{}:timeout", self.prefix, self.tenant_id)
    }

    /// Generate the Redis key for statistics.
    fn stats_key(&self) -> String {
        format!("{}:{}:stats", self.prefix, self.tenant_id)
    }

    /// Convert pool error to ACK backend error.
    fn map_error(err: PoolError) -> AckBackendError {
        match err {
            PoolError::Redis(e) => AckBackendError::Redis(e),
            PoolError::CircuitOpen => {
                AckBackendError::Unavailable("Circuit breaker is open".to_string())
            }
            PoolError::ConnectionUnavailable(msg) => AckBackendError::Unavailable(msg),
        }
    }
}

#[async_trait]
impl AckTrackerBackend for RedisAckBackend {
    fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    fn timeout_seconds(&self) -> u64 {
        self.config.timeout_seconds
    }

    fn cleanup_interval_seconds(&self) -> u64 {
        self.config.cleanup_interval_seconds
    }

    async fn track(&self, notification_id: Uuid, user_id: &str, connection_id: Uuid) {
        if !self.config.enabled {
            return;
        }

        let pending = PendingAckInfo::new(notification_id, user_id.to_string(), connection_id);
        let pending_key = self.pending_key(&notification_id);
        let timeout_key = self.timeout_key();
        let stats_key = self.stats_key();

        // Serialize the pending info
        let pending_json = match serde_json::to_string(&pending) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    notification_id = %notification_id,
                    "Failed to serialize pending ACK info"
                );
                return;
            }
        };

        // Calculate expiry timestamp
        let expiry_timestamp = Utc::now().timestamp() + self.config.timeout_seconds as i64;

        // Store pending ACK info in Hash
        let fields = [("data", pending_json.as_str())];
        if let Err(e) = self.pool.hset_multiple(&pending_key, &fields).await {
            tracing::warn!(
                error = %Self::map_error(e),
                notification_id = %notification_id,
                "Failed to store pending ACK in Redis"
            );
            return;
        }

        // Set expiration on the pending key (TTL slightly longer than timeout)
        let ttl = (self.config.timeout_seconds as i64) + 60; // Extra minute buffer
        if let Err(e) = self.pool.expire(&pending_key, ttl).await {
            tracing::warn!(
                error = %Self::map_error(e),
                notification_id = %notification_id,
                "Failed to set TTL on pending ACK key"
            );
        }

        // Add to timeout sorted set for cleanup
        let member = notification_id.to_string();
        if let Err(e) = self
            .pool
            .zadd(&timeout_key, expiry_timestamp as f64, &member)
            .await
        {
            tracing::warn!(
                error = %Self::map_error(e),
                notification_id = %notification_id,
                "Failed to add to timeout set"
            );
        }

        // Update stats
        if let Err(e) = self.pool.hincrby(&stats_key, "total_tracked", 1).await {
            tracing::debug!(
                error = %Self::map_error(e),
                "Failed to update ACK stats"
            );
        }

        ACK_TRACKED_TOTAL.inc();

        tracing::trace!(
            notification_id = %notification_id,
            user_id = %user_id,
            connection_id = %connection_id,
            "Tracking notification for ACK in Redis"
        );
    }

    async fn acknowledge(&self, notification_id: Uuid, user_id: &str) -> bool {
        if !self.config.enabled {
            return false;
        }

        let pending_key = self.pending_key(&notification_id);
        let timeout_key = self.timeout_key();
        let stats_key = self.stats_key();

        // Get pending ACK info
        let pending_json = match self.pool.hget(&pending_key, "data").await {
            Ok(Some(json)) => json,
            Ok(None) => {
                tracing::debug!(
                    notification_id = %notification_id,
                    user_id = %user_id,
                    "ACK received for unknown notification"
                );
                return false;
            }
            Err(e) => {
                tracing::warn!(
                    error = %Self::map_error(e),
                    notification_id = %notification_id,
                    "Failed to get pending ACK from Redis"
                );
                return false;
            }
        };

        // Deserialize and validate
        let pending: PendingAckInfo = match serde_json::from_str(&pending_json) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    notification_id = %notification_id,
                    "Failed to deserialize pending ACK"
                );
                return false;
            }
        };

        // Verify user_id matches
        if pending.user_id != user_id {
            tracing::warn!(
                notification_id = %notification_id,
                expected_user = %pending.user_id,
                actual_user = %user_id,
                "ACK user mismatch"
            );
            return false;
        }

        // Calculate latency before deleting
        let latency_ms = pending.latency_ms();

        // Delete pending ACK info
        if let Err(e) = self.pool.del(&pending_key).await {
            tracing::warn!(
                error = %Self::map_error(e),
                notification_id = %notification_id,
                "Failed to delete pending ACK from Redis"
            );
            // Continue anyway since ACK was valid
        }

        // Remove from timeout sorted set
        let member = notification_id.to_string();
        if let Err(e) = self.pool.zrem(&timeout_key, &member).await {
            tracing::warn!(
                error = %Self::map_error(e),
                notification_id = %notification_id,
                "Failed to remove from timeout set"
            );
        }

        // Update stats
        let _ = self.pool.hincrby(&stats_key, "total_acked", 1).await;
        let _ = self
            .pool
            .hincrby(&stats_key, "total_latency_ms", latency_ms as i64)
            .await;

        ACK_RECEIVED_TOTAL.inc();
        ACK_LATENCY.observe(latency_ms as f64 / 1000.0);

        tracing::debug!(
            notification_id = %notification_id,
            user_id = %user_id,
            latency_ms = latency_ms,
            "Notification acknowledged (Redis)"
        );

        true
    }

    async fn get_pending(
        &self,
        notification_id: Uuid,
    ) -> Result<Option<PendingAckInfo>, AckBackendError> {
        let pending_key = self.pending_key(&notification_id);

        let pending_json = self
            .pool
            .hget(&pending_key, "data")
            .await
            .map_err(Self::map_error)?;

        match pending_json {
            Some(json) => {
                let pending: PendingAckInfo = serde_json::from_str(&json)?;
                Ok(Some(pending))
            }
            None => Ok(None),
        }
    }

    async fn cleanup_expired(&self) -> usize {
        if !self.config.enabled {
            return 0;
        }

        let timeout_key = self.timeout_key();
        let stats_key = self.stats_key();

        // Get all notification IDs that have expired
        let now = Utc::now().timestamp() as f64;

        let expired_ids = match self.pool.zrangebyscore(&timeout_key, 0.0, now).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(
                    error = %Self::map_error(e),
                    "Failed to get expired ACKs from Redis"
                );
                return 0;
            }
        };

        if expired_ids.is_empty() {
            return 0;
        }

        let mut cleaned_count = 0;

        for notification_id_str in &expired_ids {
            // Parse notification ID
            let notification_id = match Uuid::parse_str(notification_id_str) {
                Ok(id) => id,
                Err(_) => continue,
            };

            // Delete pending ACK info
            let pending_key = self.pending_key(&notification_id);
            if self.pool.del(&pending_key).await.is_ok() {
                cleaned_count += 1;
            }

            // Remove from timeout set
            let _ = self.pool.zrem(&timeout_key, notification_id_str).await;
        }

        if cleaned_count > 0 {
            // Update stats
            let _ = self
                .pool
                .hincrby(&stats_key, "total_expired", cleaned_count as i64)
                .await;

            ACK_EXPIRED_TOTAL.inc_by(cleaned_count as u64);

            tracing::debug!(
                expired = cleaned_count,
                "Cleaned up expired pending ACKs from Redis"
            );
        }

        cleaned_count
    }

    async fn pending_count(&self) -> usize {
        let timeout_key = self.timeout_key();

        // Use ZCARD to count pending ACKs (efficient O(1) operation)
        match self.pool.zrangebyscore(&timeout_key, f64::MIN, f64::MAX).await {
            Ok(members) => members.len(),
            Err(e) => {
                tracing::debug!(
                    error = %Self::map_error(e),
                    "Failed to get pending count from Redis"
                );
                0
            }
        }
    }

    async fn stats(&self) -> AckBackendStats {
        let stats_key = self.stats_key();

        // Get stats from Redis
        let stats_values: Vec<(String, String)> =
            self.pool.hgetall(&stats_key).await.unwrap_or_default();

        // Parse stats values
        let mut total_tracked: u64 = 0;
        let mut total_acked: u64 = 0;
        let mut total_expired: u64 = 0;
        let mut total_latency_ms: u64 = 0;

        for (field, value) in stats_values {
            match field.as_str() {
                "total_tracked" => total_tracked = value.parse().unwrap_or(0),
                "total_acked" => total_acked = value.parse().unwrap_or(0),
                "total_expired" => total_expired = value.parse().unwrap_or(0),
                "total_latency_ms" => total_latency_ms = value.parse().unwrap_or(0),
                _ => {}
            }
        }

        let pending_count = self.pending_count().await as u64;

        AckBackendStats {
            backend_type: "redis".to_string(),
            enabled: self.config.enabled,
            total_tracked,
            total_acked,
            total_expired,
            pending_count,
            ack_rate: AckBackendStats::calculate_ack_rate(total_acked, total_expired),
            avg_latency_ms: AckBackendStats::calculate_avg_latency(total_latency_ms, total_acked),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> AckConfig {
        AckConfig {
            enabled: true,
            timeout_seconds: 30,
            cleanup_interval_seconds: 60,
        }
    }

    #[test]
    fn test_pending_key_generation() {
        let config = create_test_config();
        let pool = create_mock_pool();
        let backend = RedisAckBackend::new(config, pool, "ara:ack".to_string());

        let notif_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(
            backend.pending_key(&notif_id),
            "ara:ack:default:pending:550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_timeout_key_generation() {
        let config = create_test_config();
        let pool = create_mock_pool();
        let backend = RedisAckBackend::new(config, pool, "ara:ack".to_string());

        assert_eq!(backend.timeout_key(), "ara:ack:default:timeout");
    }

    #[test]
    fn test_stats_key_generation() {
        let config = create_test_config();
        let pool = create_mock_pool();
        let backend = RedisAckBackend::new(config, pool, "ara:ack".to_string());

        assert_eq!(backend.stats_key(), "ara:ack:default:stats");
    }

    #[test]
    fn test_key_with_tenant() {
        let config = create_test_config();
        let pool = create_mock_pool();
        let backend = RedisAckBackend::with_tenant(
            config,
            pool,
            "ara:ack".to_string(),
            "tenant-xyz".to_string(),
        );

        let notif_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(
            backend.pending_key(&notif_id),
            "ara:ack:tenant-xyz:pending:550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(backend.timeout_key(), "ara:ack:tenant-xyz:timeout");
        assert_eq!(backend.stats_key(), "ara:ack:tenant-xyz:stats");
    }

    #[test]
    fn test_is_enabled() {
        let mut config = create_test_config();
        let pool = create_mock_pool();

        config.enabled = true;
        let backend = RedisAckBackend::new(config.clone(), pool.clone(), "ara:ack".to_string());
        assert!(backend.is_enabled());

        config.enabled = false;
        let backend = RedisAckBackend::new(config, pool, "ara:ack".to_string());
        assert!(!backend.is_enabled());
    }

    #[test]
    fn test_timeout_seconds() {
        let mut config = create_test_config();
        config.timeout_seconds = 45;
        let pool = create_mock_pool();
        let backend = RedisAckBackend::new(config, pool, "ara:ack".to_string());

        assert_eq!(backend.timeout_seconds(), 45);
    }

    #[test]
    fn test_cleanup_interval_seconds() {
        let mut config = create_test_config();
        config.cleanup_interval_seconds = 120;
        let pool = create_mock_pool();
        let backend = RedisAckBackend::new(config, pool, "ara:ack".to_string());

        assert_eq!(backend.cleanup_interval_seconds(), 120);
    }

    fn create_mock_pool() -> Arc<RedisPool> {
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

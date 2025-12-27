//! In-memory message queue backend using DashMap.
//!
//! This module provides a memory-based implementation of the `MessageQueueBackend` trait.
//! Messages are stored in memory and will be lost on service restart.

use std::collections::VecDeque;

use async_trait::async_trait;
use dashmap::DashMap;

use crate::metrics::{QUEUE_DROPPED_TOTAL, QUEUE_ENQUEUED_TOTAL, QUEUE_EXPIRED_TOTAL};
use crate::notification::NotificationEvent;

use super::backend::{
    DrainResult, MessageQueueBackend, QueueBackendError, QueueBackendStats, StoredMessage,
};
use super::QueueConfig;

/// In-memory message queue backend.
///
/// Uses `DashMap` for concurrent access to per-user queues.
/// Each user has a `VecDeque` acting as a circular buffer.
/// When queue is full, oldest messages are dropped (FIFO).
pub struct MemoryQueueBackend {
    /// Per-user message queues
    queues: DashMap<String, VecDeque<StoredMessage>>,
    /// Configuration
    config: QueueConfig,
}

impl MemoryQueueBackend {
    /// Create a new memory queue backend with the given configuration.
    pub fn new(config: QueueConfig) -> Self {
        Self {
            queues: DashMap::new(),
            config,
        }
    }
}

#[async_trait]
impl MessageQueueBackend for MemoryQueueBackend {
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

        let mut queue = self.queues.entry(user_id.to_string()).or_default();

        // If queue is full, remove oldest message
        if queue.len() >= self.config.max_queue_size_per_user {
            if let Some(dropped) = queue.pop_front() {
                QUEUE_DROPPED_TOTAL.inc();
                tracing::debug!(
                    user_id = %user_id,
                    dropped_id = %dropped.id,
                    queue_size = queue.len(),
                    "Dropped oldest message from full queue"
                );
            }
        }

        queue.push_back(message);
        QUEUE_ENQUEUED_TOTAL.inc();

        tracing::debug!(
            user_id = %user_id,
            queue_size = queue.len(),
            "Message enqueued for offline user"
        );

        Ok(())
    }

    async fn drain(&self, user_id: &str) -> Result<DrainResult, QueueBackendError> {
        if !self.config.enabled {
            return Ok(DrainResult::default());
        }

        // Take ownership of the queue for this user
        let messages = match self.queues.remove(user_id) {
            Some((_, queue)) => queue,
            None => return Ok(DrainResult::default()),
        };

        if messages.is_empty() {
            return Ok(DrainResult::default());
        }

        let ttl = self.config.message_ttl_seconds;
        let mut valid_messages = Vec::new();
        let mut expired = 0;

        for message in messages {
            if message.is_expired(ttl) {
                expired += 1;
                QUEUE_EXPIRED_TOTAL.inc();
                tracing::debug!(
                    user_id = %user_id,
                    message_id = %message.id,
                    queued_at = %message.queued_at,
                    "Discarding expired message during drain"
                );
            } else {
                valid_messages.push(message);
            }
        }

        tracing::info!(
            user_id = %user_id,
            message_count = valid_messages.len(),
            expired = expired,
            "Drained message queue for user"
        );

        Ok(DrainResult {
            messages: valid_messages,
            expired,
        })
    }

    async fn peek(&self, user_id: &str, limit: usize) -> Result<Vec<StoredMessage>, QueueBackendError> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let messages = self
            .queues
            .get(user_id)
            .map(|q| q.iter().take(limit).cloned().collect())
            .unwrap_or_default();

        Ok(messages)
    }

    async fn queue_size(&self, user_id: &str) -> Result<usize, QueueBackendError> {
        Ok(self.queues.get(user_id).map(|q| q.len()).unwrap_or(0))
    }

    async fn cleanup_expired(&self) -> Result<usize, QueueBackendError> {
        let ttl = self.config.message_ttl_seconds;
        let mut removed = 0;

        // Collect user IDs first to avoid holding locks
        let user_ids: Vec<String> = self.queues.iter().map(|r| r.key().clone()).collect();

        for user_id in user_ids {
            if let Some(mut queue) = self.queues.get_mut(&user_id) {
                let before = queue.len();
                queue.retain(|msg| !msg.is_expired(ttl));
                let after = queue.len();
                let expired = before - after;
                removed += expired;

                // Update Prometheus metrics for expired messages
                if expired > 0 {
                    QUEUE_EXPIRED_TOTAL.inc_by(expired as u64);
                }

                // Remove empty queues
                if queue.is_empty() {
                    drop(queue);
                    self.queues.remove(&user_id);
                }
            }
        }

        if removed > 0 {
            tracing::info!(
                removed = removed,
                remaining_users = self.queues.len(),
                "Cleaned up expired messages"
            );
        }

        Ok(removed)
    }

    async fn clear_user_queue(&self, user_id: &str) -> Result<usize, QueueBackendError> {
        Ok(self.queues.remove(user_id).map(|(_, q)| q.len()).unwrap_or(0))
    }

    async fn stats(&self) -> QueueBackendStats {
        let mut total_messages = 0;
        let mut users_with_queue = 0;
        let mut max_queue_size = 0;

        for entry in self.queues.iter() {
            let size = entry.len();
            total_messages += size;
            users_with_queue += 1;
            max_queue_size = max_queue_size.max(size);
        }

        QueueBackendStats {
            backend_type: "memory".to_string(),
            enabled: self.config.enabled,
            total_messages,
            users_with_queue,
            max_queue_size,
            max_queue_size_config: self.config.max_queue_size_per_user,
            message_ttl_seconds: self.config.message_ttl_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_event() -> NotificationEvent {
        NotificationEvent::builder("test.event", "test-source")
            .payload(json!({"key": "value"}))
            .build()
    }

    fn create_enabled_config() -> QueueConfig {
        QueueConfig {
            enabled: true,
            max_queue_size_per_user: 10,
            message_ttl_seconds: 3600,
            cleanup_interval_seconds: 300,
        }
    }

    #[tokio::test]
    async fn test_enqueue_when_disabled() {
        let config = QueueConfig {
            enabled: false,
            ..Default::default()
        };
        let backend = MemoryQueueBackend::new(config);
        let event = create_test_event();

        let result = backend.enqueue("user-1", event).await;
        assert!(matches!(result, Err(QueueBackendError::Disabled)));
    }

    #[tokio::test]
    async fn test_enqueue_success() {
        let backend = MemoryQueueBackend::new(create_enabled_config());

        for _ in 0..5 {
            let event = create_test_event();
            backend.enqueue("user-1", event).await.unwrap();
        }

        assert_eq!(backend.queue_size("user-1").await.unwrap(), 5);
    }

    #[tokio::test]
    async fn test_enqueue_drops_oldest_when_full() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 3,
            ..Default::default()
        };
        let backend = MemoryQueueBackend::new(config);

        // Add 5 messages to a queue that can only hold 3
        for _ in 0..5 {
            let event = create_test_event();
            backend.enqueue("user-1", event).await.unwrap();
        }

        // Should only have 3 messages (the newest ones)
        assert_eq!(backend.queue_size("user-1").await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_drain_empty_queue() {
        let backend = MemoryQueueBackend::new(create_enabled_config());

        let result = backend.drain("user-1").await.unwrap();
        assert!(result.messages.is_empty());
        assert_eq!(result.expired, 0);
    }

    #[tokio::test]
    async fn test_drain_success() {
        let backend = MemoryQueueBackend::new(create_enabled_config());

        // Enqueue messages
        for _ in 0..3 {
            backend.enqueue("user-1", create_test_event()).await.unwrap();
        }

        // Drain
        let result = backend.drain("user-1").await.unwrap();

        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.expired, 0);

        // Queue should be empty after drain
        assert_eq!(backend.queue_size("user-1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_drain_filters_expired() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 100,
            message_ttl_seconds: 0, // Immediate expiry
            cleanup_interval_seconds: 300,
        };
        let backend = MemoryQueueBackend::new(config);

        // Enqueue messages
        for _ in 0..3 {
            backend.enqueue("user-1", create_test_event()).await.unwrap();
        }

        // Drain - all should be expired
        let result = backend.drain("user-1").await.unwrap();

        assert!(result.messages.is_empty());
        assert_eq!(result.expired, 3);
    }

    #[tokio::test]
    async fn test_peek() {
        let backend = MemoryQueueBackend::new(create_enabled_config());

        for _ in 0..5 {
            backend.enqueue("user-1", create_test_event()).await.unwrap();
        }

        // Peek at first 3
        let messages = backend.peek("user-1", 3).await.unwrap();
        assert_eq!(messages.len(), 3);

        // Queue should still have all 5
        assert_eq!(backend.queue_size("user-1").await.unwrap(), 5);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 100,
            message_ttl_seconds: 0, // Immediate expiry
            cleanup_interval_seconds: 300,
        };
        let backend = MemoryQueueBackend::new(config);

        // Add messages
        for _ in 0..5 {
            backend.enqueue("user-1", create_test_event()).await.unwrap();
        }

        // All should be expired immediately
        let removed = backend.cleanup_expired().await.unwrap();
        assert_eq!(removed, 5);
        assert_eq!(backend.queue_size("user-1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_clear_user_queue() {
        let backend = MemoryQueueBackend::new(create_enabled_config());

        for _ in 0..5 {
            backend.enqueue("user-1", create_test_event()).await.unwrap();
        }

        let cleared = backend.clear_user_queue("user-1").await.unwrap();
        assert_eq!(cleared, 5);
        assert_eq!(backend.queue_size("user-1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_multiple_users() {
        let backend = MemoryQueueBackend::new(create_enabled_config());

        for _ in 0..3 {
            backend.enqueue("user-1", create_test_event()).await.unwrap();
        }
        for _ in 0..5 {
            backend.enqueue("user-2", create_test_event()).await.unwrap();
        }

        assert_eq!(backend.queue_size("user-1").await.unwrap(), 3);
        assert_eq!(backend.queue_size("user-2").await.unwrap(), 5);

        let stats = backend.stats().await;
        assert_eq!(stats.total_messages, 8);
        assert_eq!(stats.users_with_queue, 2);
    }

    #[tokio::test]
    async fn test_stats() {
        let backend = MemoryQueueBackend::new(create_enabled_config());

        for _ in 0..3 {
            backend.enqueue("user-1", create_test_event()).await.unwrap();
        }
        for _ in 0..7 {
            backend.enqueue("user-2", create_test_event()).await.unwrap();
        }

        let stats = backend.stats().await;
        assert_eq!(stats.backend_type, "memory");
        assert!(stats.enabled);
        assert_eq!(stats.total_messages, 10);
        assert_eq!(stats.users_with_queue, 2);
        assert_eq!(stats.max_queue_size, 7);
        assert_eq!(stats.max_queue_size_config, 10);
        assert_eq!(stats.message_ttl_seconds, 3600);
    }
}

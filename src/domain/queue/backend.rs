//! Backend trait for message queue storage.
//!
//! This module defines the abstraction layer for message queue backends,
//! allowing different storage implementations (memory, Redis, etc.) to be
//! used interchangeably.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::notification::NotificationEvent;

/// Errors that can occur during queue backend operations.
#[derive(Debug, Error)]
pub enum QueueBackendError {
    /// Queue is disabled
    #[error("Message queue is disabled")]
    Disabled,

    /// Queue is full for this user
    #[error("Queue full for user {user_id} (size: {size})")]
    QueueFull { user_id: String, size: usize },

    /// Redis operation failed
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    /// PostgreSQL operation failed
    #[error("PostgreSQL error: {0}")]
    Postgres(#[from] sqlx::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Backend is temporarily unavailable (e.g., circuit breaker open)
    #[error("Backend unavailable: {0}")]
    Unavailable(String),
}

/// A message stored in the queue.
///
/// This is the serializable representation of a queued message,
/// used for both memory and Redis backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    /// Unique message ID
    pub id: Uuid,

    /// The notification event
    pub event: NotificationEvent,

    /// When the message was queued
    pub queued_at: DateTime<Utc>,

    /// Number of delivery attempts
    pub attempts: u32,

    /// Redis stream ID (only set for Redis backend)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
}

impl StoredMessage {
    /// Create a new stored message from a notification event.
    pub fn new(event: NotificationEvent) -> Self {
        Self {
            id: Uuid::new_v4(),
            event,
            queued_at: Utc::now(),
            attempts: 0,
            stream_id: None,
        }
    }

    /// Check if the message has expired based on the given TTL.
    pub fn is_expired(&self, ttl_seconds: u64) -> bool {
        let now = Utc::now();
        let age = now.signed_duration_since(self.queued_at);
        age.num_seconds() >= ttl_seconds as i64
    }
}

/// Result of a drain/replay operation.
#[derive(Debug, Clone, Default)]
pub struct DrainResult {
    /// Messages that were retrieved
    pub messages: Vec<StoredMessage>,

    /// Number of messages that were expired and discarded
    pub expired: usize,
}

/// Statistics about the queue backend.
#[derive(Debug, Clone, Serialize)]
pub struct QueueBackendStats {
    /// Backend type identifier
    pub backend_type: String,

    /// Whether the queue is enabled
    pub enabled: bool,

    /// Total number of messages across all users
    pub total_messages: usize,

    /// Number of users with queued messages
    pub users_with_queue: usize,

    /// Maximum queue size for any single user
    pub max_queue_size: usize,

    /// Configured maximum queue size per user
    pub max_queue_size_config: usize,

    /// Configured message TTL in seconds
    pub message_ttl_seconds: u64,
}

/// Backend trait for message queue storage.
///
/// This trait abstracts the storage layer for offline message queues,
/// allowing different implementations (memory, Redis, etc.) to be used.
///
/// # Thread Safety
///
/// Implementations must be thread-safe (`Send + Sync`) as they will be
/// shared across multiple async tasks.
///
/// # Error Handling
///
/// All fallible operations return `Result<T, QueueBackendError>`.
/// Implementations should handle transient failures gracefully and
/// log appropriate warnings.
#[async_trait]
pub trait MessageQueueBackend: Send + Sync {
    /// Check if the queue backend is enabled.
    fn is_enabled(&self) -> bool;

    /// Get the message TTL in seconds.
    fn message_ttl_seconds(&self) -> u64;

    /// Enqueue a message for a user.
    ///
    /// If the queue is full, the oldest message should be dropped to make room.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The user ID to queue the message for
    /// * `event` - The notification event to queue
    ///
    /// # Errors
    ///
    /// Returns `QueueBackendError::Disabled` if the queue is disabled.
    /// Returns `QueueBackendError::Redis` for Redis backend failures.
    async fn enqueue(&self, user_id: &str, event: NotificationEvent) -> Result<(), QueueBackendError>;

    /// Drain all messages for a user.
    ///
    /// This removes all messages from the queue and returns them.
    /// Expired messages should be filtered out and counted.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The user ID to drain messages for
    ///
    /// # Returns
    ///
    /// A `DrainResult` containing the messages and count of expired messages.
    async fn drain(&self, user_id: &str) -> Result<DrainResult, QueueBackendError>;

    /// Peek at messages without removing them.
    ///
    /// Useful for debugging and monitoring.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The user ID to peek messages for
    /// * `limit` - Maximum number of messages to return
    async fn peek(&self, user_id: &str, limit: usize) -> Result<Vec<StoredMessage>, QueueBackendError>;

    /// Get the queue size for a specific user.
    async fn queue_size(&self, user_id: &str) -> Result<usize, QueueBackendError>;

    /// Clean up expired messages from all queues.
    ///
    /// # Returns
    ///
    /// The number of messages removed.
    async fn cleanup_expired(&self) -> Result<usize, QueueBackendError>;

    /// Clear the queue for a specific user.
    ///
    /// # Returns
    ///
    /// The number of messages removed.
    async fn clear_user_queue(&self, user_id: &str) -> Result<usize, QueueBackendError>;

    /// Get queue statistics.
    async fn stats(&self) -> QueueBackendStats;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_stored_message_new() {
        let event = NotificationEvent::builder("test.event", "test-source")
            .payload(json!({"key": "value"}))
            .build();

        let msg = StoredMessage::new(event);

        assert!(!msg.id.is_nil());
        assert_eq!(msg.attempts, 0);
        assert!(msg.stream_id.is_none());
    }

    #[test]
    fn test_stored_message_not_expired() {
        let event = NotificationEvent::builder("test.event", "test-source")
            .payload(json!({}))
            .build();

        let msg = StoredMessage::new(event);

        // With 1 hour TTL, message should not be expired
        assert!(!msg.is_expired(3600));
    }

    #[test]
    fn test_stored_message_expired() {
        let event = NotificationEvent::builder("test.event", "test-source")
            .payload(json!({}))
            .build();

        let msg = StoredMessage::new(event);

        // With 0 TTL, message should be expired immediately
        assert!(msg.is_expired(0));
    }

    #[test]
    fn test_stored_message_serialization() {
        let event = NotificationEvent::builder("test.event", "test-source")
            .payload(json!({"key": "value"}))
            .build();

        let msg = StoredMessage::new(event);

        // Should serialize without stream_id
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("stream_id"));

        // Should deserialize back
        let deserialized: StoredMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, msg.id);
        assert_eq!(deserialized.attempts, msg.attempts);
    }

    #[test]
    fn test_drain_result_default() {
        let result = DrainResult::default();
        assert!(result.messages.is_empty());
        assert_eq!(result.expired, 0);
    }
}

//! Message queue module for offline message delivery.
//!
//! This module provides a per-user message queue that stores notifications
//! when users are disconnected and replays them upon reconnection.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::metrics::{QUEUE_DROPPED_TOTAL, QUEUE_ENQUEUED_TOTAL, QUEUE_EXPIRED_TOTAL, QUEUE_REPLAYED_TOTAL};
use crate::notification::NotificationEvent;
use crate::websocket::{OutboundMessage, ServerMessage};

/// Configuration for the message queue
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Whether the queue is enabled
    pub enabled: bool,
    /// Maximum number of messages to queue per user
    pub max_queue_size_per_user: usize,
    /// Time-to-live for queued messages in seconds
    pub message_ttl_seconds: u64,
    /// Interval for cleanup task in seconds
    pub cleanup_interval_seconds: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_queue_size_per_user: 100,
            message_ttl_seconds: 3600, // 1 hour
            cleanup_interval_seconds: 300, // 5 minutes
        }
    }
}

/// A message queued for later delivery
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// Unique message ID
    pub id: Uuid,
    /// The notification event
    pub event: NotificationEvent,
    /// When the message was queued
    pub queued_at: DateTime<Utc>,
    /// Number of delivery attempts
    pub attempts: u32,
}

impl QueuedMessage {
    /// Create a new queued message
    pub fn new(event: NotificationEvent) -> Self {
        Self {
            id: Uuid::new_v4(),
            event,
            queued_at: Utc::now(),
            attempts: 0,
        }
    }

    /// Check if the message has expired
    pub fn is_expired(&self, ttl_seconds: u64) -> bool {
        let now = Utc::now();
        let age = now.signed_duration_since(self.queued_at);
        age.num_seconds() >= ttl_seconds as i64
    }
}

/// Result of a replay operation
#[derive(Debug, Clone)]
pub struct ReplayResult {
    /// Number of messages replayed successfully
    pub replayed: usize,
    /// Number of messages that failed to send
    pub failed: usize,
    /// Number of messages that were expired and discarded
    pub expired: usize,
}

impl ReplayResult {
    pub fn empty() -> Self {
        Self {
            replayed: 0,
            failed: 0,
            expired: 0,
        }
    }
}

/// Error types for queue operations
#[derive(Debug, Clone)]
pub enum QueueError {
    /// Queue is disabled
    Disabled,
    /// Queue is full for this user
    QueueFull { user_id: String, size: usize },
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "Message queue is disabled"),
            Self::QueueFull { user_id, size } => {
                write!(f, "Queue full for user {} (size: {})", user_id, size)
            }
        }
    }
}

impl std::error::Error for QueueError {}

/// Per-user message queue for offline delivery.
///
/// When a user disconnects, messages are stored in their queue.
/// Upon reconnection, all queued messages are replayed to the user.
///
/// # Design
///
/// - Uses `DashMap` for concurrent access to per-user queues
/// - Each user has a `VecDeque` acting as a circular buffer
/// - When queue is full, oldest messages are dropped (FIFO)
/// - Expired messages are automatically cleaned up
///
/// # Example
///
/// ```rust,ignore
/// let queue = UserMessageQueue::new(QueueConfig::default());
///
/// // Queue a message when user is offline
/// queue.enqueue("user-123", event)?;
///
/// // Replay when user reconnects
/// let result = queue.replay("user-123", &connection_handle).await;
/// ```
pub struct UserMessageQueue {
    /// Per-user message queues
    queues: DashMap<String, VecDeque<QueuedMessage>>,
    /// Configuration
    config: QueueConfig,
}

impl UserMessageQueue {
    /// Create a new message queue with the given configuration
    pub fn new(config: QueueConfig) -> Self {
        Self {
            queues: DashMap::new(),
            config,
        }
    }

    /// Check if the queue is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the configuration
    pub fn config(&self) -> &QueueConfig {
        &self.config
    }

    /// Enqueue a message for a user.
    ///
    /// If the queue is full, the oldest message is dropped to make room.
    /// Returns an error if the queue is disabled.
    pub fn enqueue(&self, user_id: &str, event: NotificationEvent) -> Result<(), QueueError> {
        if !self.config.enabled {
            return Err(QueueError::Disabled);
        }

        let message = QueuedMessage::new(event);

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

    /// Replay all queued messages to a user's connection.
    ///
    /// Messages are sent in order (oldest first) and removed from the queue
    /// upon successful delivery. Expired messages are discarded.
    pub async fn replay(
        &self,
        user_id: &str,
        sender: &mpsc::Sender<OutboundMessage>,
    ) -> ReplayResult {
        if !self.config.enabled {
            return ReplayResult::empty();
        }

        // Take ownership of the queue for this user
        let messages = match self.queues.remove(user_id) {
            Some((_, queue)) => queue,
            None => return ReplayResult::empty(),
        };

        if messages.is_empty() {
            return ReplayResult::empty();
        }

        let total = messages.len();
        let mut replayed = 0;
        let mut failed = 0;
        let mut expired = 0;

        tracing::info!(
            user_id = %user_id,
            message_count = total,
            "Starting message replay for reconnected user"
        );

        for message in messages {
            // Skip expired messages
            if message.is_expired(self.config.message_ttl_seconds) {
                expired += 1;
                QUEUE_EXPIRED_TOTAL.inc();
                tracing::debug!(
                    user_id = %user_id,
                    message_id = %message.id,
                    queued_at = %message.queued_at,
                    "Discarding expired message"
                );
                continue;
            }

            // Send the message
            let server_msg = ServerMessage::Notification {
                event: message.event,
            };

            match sender.send(OutboundMessage::Raw(server_msg)).await {
                Ok(_) => {
                    replayed += 1;
                    QUEUE_REPLAYED_TOTAL.inc();
                }
                Err(_) => {
                    failed += 1;
                    tracing::warn!(
                        user_id = %user_id,
                        message_id = %message.id,
                        "Failed to replay message, connection may be closed"
                    );
                    // If sending fails, stop replaying (connection is dead)
                    break;
                }
            }
        }

        tracing::info!(
            user_id = %user_id,
            replayed = replayed,
            failed = failed,
            expired = expired,
            "Message replay completed"
        );

        ReplayResult {
            replayed,
            failed,
            expired,
        }
    }

    /// Get the number of queued messages for a user
    pub fn queue_size(&self, user_id: &str) -> usize {
        self.queues
            .get(user_id)
            .map(|q| q.len())
            .unwrap_or(0)
    }

    /// Get the total number of queued messages across all users
    pub fn total_queued(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    /// Get the number of users with queued messages
    pub fn users_with_queue(&self) -> usize {
        self.queues.len()
    }

    /// Clean up expired messages from all queues.
    ///
    /// Returns the number of messages removed.
    pub fn cleanup_expired(&self) -> usize {
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

        removed
    }

    /// Clear the queue for a specific user.
    ///
    /// This is called when all of a user's connections successfully receive
    /// the replayed messages.
    pub fn clear_user_queue(&self, user_id: &str) -> usize {
        match self.queues.remove(user_id) {
            Some((_, queue)) => queue.len(),
            None => 0,
        }
    }

    /// Get queue statistics
    pub fn stats(&self) -> QueueStats {
        let mut total_messages = 0;
        let mut users_with_queue = 0;
        let mut max_queue_size = 0;

        for entry in self.queues.iter() {
            let size = entry.len();
            total_messages += size;
            users_with_queue += 1;
            max_queue_size = max_queue_size.max(size);
        }

        QueueStats {
            enabled: self.config.enabled,
            total_messages,
            users_with_queue,
            max_queue_size,
            max_queue_size_config: self.config.max_queue_size_per_user,
            message_ttl_seconds: self.config.message_ttl_seconds,
        }
    }
}

/// Statistics about the message queue
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueueStats {
    pub enabled: bool,
    pub total_messages: usize,
    pub users_with_queue: usize,
    pub max_queue_size: usize,
    pub max_queue_size_config: usize,
    pub message_ttl_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::NotificationEvent;
    use serde_json::json;

    fn create_test_event() -> NotificationEvent {
        NotificationEvent::builder("test.event", "test-source")
            .payload(json!({"key": "value"}))
            .build()
    }

    #[test]
    fn test_enqueue_when_disabled() {
        let config = QueueConfig {
            enabled: false,
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);
        let event = create_test_event();

        let result = queue.enqueue("user-1", event);
        assert!(matches!(result, Err(QueueError::Disabled)));
    }

    #[test]
    fn test_enqueue_success() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 10,
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);

        for _ in 0..5 {
            let event = create_test_event();
            queue.enqueue("user-1", event).unwrap();
        }

        assert_eq!(queue.queue_size("user-1"), 5);
        assert_eq!(queue.total_queued(), 5);
        assert_eq!(queue.users_with_queue(), 1);
    }

    #[test]
    fn test_enqueue_drops_oldest_when_full() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 3,
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);

        // Add 5 messages to a queue that can only hold 3
        for _ in 0..5 {
            let event = create_test_event();
            queue.enqueue("user-1", event).unwrap();
        }

        // Should only have 3 messages (the newest ones)
        assert_eq!(queue.queue_size("user-1"), 3);
    }

    #[test]
    fn test_cleanup_expired() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 100,
            message_ttl_seconds: 0, // Immediate expiry
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);

        // Add messages
        for _ in 0..5 {
            let event = create_test_event();
            queue.enqueue("user-1", event).unwrap();
        }

        // All should be expired immediately
        let removed = queue.cleanup_expired();
        assert_eq!(removed, 5);
        assert_eq!(queue.queue_size("user-1"), 0);
        assert_eq!(queue.users_with_queue(), 0);
    }

    #[test]
    fn test_multiple_users() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 10,
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);

        // Add messages for different users
        for _ in 0..3 {
            queue.enqueue("user-1", create_test_event()).unwrap();
        }
        for _ in 0..5 {
            queue.enqueue("user-2", create_test_event()).unwrap();
        }

        assert_eq!(queue.queue_size("user-1"), 3);
        assert_eq!(queue.queue_size("user-2"), 5);
        assert_eq!(queue.total_queued(), 8);
        assert_eq!(queue.users_with_queue(), 2);
    }

    #[tokio::test]
    async fn test_replay_empty_queue() {
        let config = QueueConfig {
            enabled: true,
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);
        let (tx, _rx) = mpsc::channel(10);

        let result = queue.replay("user-1", &tx).await;
        assert_eq!(result.replayed, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.expired, 0);
    }

    #[tokio::test]
    async fn test_replay_success() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 100,
            message_ttl_seconds: 3600,
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);

        // Enqueue messages
        for _ in 0..3 {
            queue.enqueue("user-1", create_test_event()).unwrap();
        }

        // Create a channel to receive replayed messages
        let (tx, mut rx) = mpsc::channel(10);

        // Replay
        let result = queue.replay("user-1", &tx).await;

        assert_eq!(result.replayed, 3);
        assert_eq!(result.failed, 0);
        assert_eq!(result.expired, 0);

        // Queue should be empty after replay
        assert_eq!(queue.queue_size("user-1"), 0);

        // Should have received 3 messages
        let mut received = 0;
        while rx.try_recv().is_ok() {
            received += 1;
        }
        assert_eq!(received, 3);
    }

    #[test]
    fn test_clear_user_queue() {
        let config = QueueConfig {
            enabled: true,
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);

        for _ in 0..5 {
            queue.enqueue("user-1", create_test_event()).unwrap();
        }

        let cleared = queue.clear_user_queue("user-1");
        assert_eq!(cleared, 5);
        assert_eq!(queue.queue_size("user-1"), 0);
    }

    #[test]
    fn test_stats() {
        let config = QueueConfig {
            enabled: true,
            max_queue_size_per_user: 100,
            message_ttl_seconds: 3600,
            ..Default::default()
        };
        let queue = UserMessageQueue::new(config);

        for _ in 0..3 {
            queue.enqueue("user-1", create_test_event()).unwrap();
        }
        for _ in 0..7 {
            queue.enqueue("user-2", create_test_event()).unwrap();
        }

        let stats = queue.stats();
        assert!(stats.enabled);
        assert_eq!(stats.total_messages, 10);
        assert_eq!(stats.users_with_queue, 2);
        assert_eq!(stats.max_queue_size, 7);
        assert_eq!(stats.max_queue_size_config, 100);
        assert_eq!(stats.message_ttl_seconds, 3600);
    }
}

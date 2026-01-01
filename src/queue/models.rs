//! Queue data models and error types

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::notification::NotificationEvent;

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
            message_ttl_seconds: 3600,    // 1 hour
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

/// Statistics about the message queue
#[derive(Debug, Clone, Serialize)]
pub struct QueueStats {
    pub enabled: bool,
    pub total_messages: usize,
    pub users_with_queue: usize,
    pub max_queue_size: usize,
    pub max_queue_size_config: usize,
    pub message_ttl_seconds: u64,
}

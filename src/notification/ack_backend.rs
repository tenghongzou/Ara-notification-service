//! Backend trait for ACK tracking storage.
//!
//! This module defines the abstraction layer for ACK tracking backends,
//! allowing different storage implementations (memory, Redis, etc.) to be
//! used interchangeably.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during ACK backend operations.
#[derive(Debug, Error)]
pub enum AckBackendError {
    /// ACK tracking is disabled
    #[error("ACK tracking is disabled")]
    Disabled,

    /// Redis operation failed
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Backend is temporarily unavailable (e.g., circuit breaker open)
    #[error("Backend unavailable: {0}")]
    Unavailable(String),
}

/// Information about a pending ACK.
///
/// This is the serializable representation of a pending acknowledgment,
/// used for both memory and Redis backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAckInfo {
    /// The notification ID
    pub notification_id: Uuid,

    /// The user ID who should acknowledge
    pub user_id: String,

    /// Connection ID that received the notification
    pub connection_id: Uuid,

    /// Timestamp when the notification was sent
    pub sent_at: DateTime<Utc>,
}

impl PendingAckInfo {
    /// Create a new pending ACK info.
    pub fn new(notification_id: Uuid, user_id: String, connection_id: Uuid) -> Self {
        Self {
            notification_id,
            user_id,
            connection_id,
            sent_at: Utc::now(),
        }
    }

    /// Check if this pending ACK has expired based on the given timeout.
    pub fn is_expired(&self, timeout_seconds: u64) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.sent_at);
        elapsed.num_seconds() >= timeout_seconds as i64
    }

    /// Calculate the latency in milliseconds since the notification was sent.
    pub fn latency_ms(&self) -> u64 {
        Utc::now()
            .signed_duration_since(self.sent_at)
            .num_milliseconds()
            .max(0) as u64
    }
}

/// Statistics snapshot for ACK tracking.
#[derive(Debug, Clone, Serialize)]
pub struct AckBackendStats {
    /// Backend type identifier
    pub backend_type: String,

    /// Whether ACK tracking is enabled
    pub enabled: bool,

    /// Total notifications tracked for ACK
    pub total_tracked: u64,

    /// Total ACKs received
    pub total_acked: u64,

    /// Total expired (unacknowledged) notifications
    pub total_expired: u64,

    /// Current pending ACKs count
    pub pending_count: u64,

    /// ACK rate (acked / (acked + expired))
    pub ack_rate: f64,

    /// Average ACK latency in milliseconds
    pub avg_latency_ms: u64,
}

impl AckBackendStats {
    /// Calculate ACK rate from acked and expired counts.
    pub fn calculate_ack_rate(acked: u64, expired: u64) -> f64 {
        let completed = acked + expired;
        if completed > 0 {
            acked as f64 / completed as f64
        } else {
            1.0 // No completions yet, assume 100%
        }
    }

    /// Calculate average latency from total latency and ack count.
    pub fn calculate_avg_latency(total_latency_ms: u64, ack_count: u64) -> u64 {
        if ack_count > 0 {
            total_latency_ms / ack_count
        } else {
            0
        }
    }
}

/// Backend trait for ACK tracking storage.
///
/// This trait abstracts the storage layer for ACK tracking,
/// allowing different implementations (memory, Redis, etc.) to be used.
///
/// # Thread Safety
///
/// Implementations must be thread-safe (`Send + Sync`) as they will be
/// shared across multiple async tasks.
///
/// # Error Handling
///
/// Most operations return `Result<T, AckBackendError>`, but some operations
/// like `track` and `acknowledge` are designed to be best-effort and
/// return simple types instead of Results to avoid disrupting the main
/// notification flow.
#[async_trait]
pub trait AckTrackerBackend: Send + Sync {
    /// Check if ACK tracking is enabled.
    fn is_enabled(&self) -> bool;

    /// Get the ACK timeout in seconds.
    fn timeout_seconds(&self) -> u64;

    /// Get the cleanup interval in seconds.
    fn cleanup_interval_seconds(&self) -> u64;

    /// Track a notification for ACK.
    ///
    /// This should be called when a notification is sent to a connection.
    /// The operation is best-effort and should not block the notification flow.
    ///
    /// # Arguments
    ///
    /// * `notification_id` - The unique notification ID
    /// * `user_id` - The user ID who should acknowledge
    /// * `connection_id` - The connection ID that received the notification
    async fn track(&self, notification_id: Uuid, user_id: &str, connection_id: Uuid);

    /// Acknowledge a notification.
    ///
    /// Returns `true` if the ACK was valid (notification was pending and
    /// user ID matches), `false` otherwise.
    ///
    /// # Arguments
    ///
    /// * `notification_id` - The notification ID to acknowledge
    /// * `user_id` - The user ID sending the ACK
    async fn acknowledge(&self, notification_id: Uuid, user_id: &str) -> bool;

    /// Get pending ACK info for a notification.
    ///
    /// Useful for debugging and validation.
    async fn get_pending(&self, notification_id: Uuid) -> Result<Option<PendingAckInfo>, AckBackendError>;

    /// Clean up expired pending ACKs.
    ///
    /// # Returns
    ///
    /// The number of expired ACKs removed.
    async fn cleanup_expired(&self) -> usize;

    /// Get the current pending ACK count.
    async fn pending_count(&self) -> usize;

    /// Get ACK tracking statistics.
    async fn stats(&self) -> AckBackendStats;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_ack_info_new() {
        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        let info = PendingAckInfo::new(notif_id, "user-123".to_string(), conn_id);

        assert_eq!(info.notification_id, notif_id);
        assert_eq!(info.user_id, "user-123");
        assert_eq!(info.connection_id, conn_id);
    }

    #[test]
    fn test_pending_ack_info_not_expired() {
        let info = PendingAckInfo::new(
            Uuid::new_v4(),
            "user-123".to_string(),
            Uuid::new_v4(),
        );

        // With 30 second timeout, should not be expired
        assert!(!info.is_expired(30));
    }

    #[test]
    fn test_pending_ack_info_expired() {
        let info = PendingAckInfo::new(
            Uuid::new_v4(),
            "user-123".to_string(),
            Uuid::new_v4(),
        );

        // With 0 timeout, should be expired immediately
        assert!(info.is_expired(0));
    }

    #[test]
    fn test_pending_ack_info_latency() {
        let info = PendingAckInfo::new(
            Uuid::new_v4(),
            "user-123".to_string(),
            Uuid::new_v4(),
        );

        // Latency should be very small immediately after creation
        assert!(info.latency_ms() < 100);
    }

    #[test]
    fn test_pending_ack_info_serialization() {
        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        let info = PendingAckInfo::new(notif_id, "user-123".to_string(), conn_id);

        // Should serialize to JSON
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("user-123"));

        // Should deserialize back
        let deserialized: PendingAckInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.notification_id, notif_id);
        assert_eq!(deserialized.user_id, "user-123");
        assert_eq!(deserialized.connection_id, conn_id);
    }

    #[test]
    fn test_ack_backend_stats_calculate_ack_rate() {
        // No completions
        assert_eq!(AckBackendStats::calculate_ack_rate(0, 0), 1.0);

        // All acked
        assert_eq!(AckBackendStats::calculate_ack_rate(10, 0), 1.0);

        // Half and half
        assert!((AckBackendStats::calculate_ack_rate(5, 5) - 0.5).abs() < 0.001);

        // Mostly expired
        assert!((AckBackendStats::calculate_ack_rate(1, 9) - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_ack_backend_stats_calculate_avg_latency() {
        // No acks
        assert_eq!(AckBackendStats::calculate_avg_latency(1000, 0), 0);

        // Some acks
        assert_eq!(AckBackendStats::calculate_avg_latency(1000, 10), 100);
        assert_eq!(AckBackendStats::calculate_avg_latency(500, 5), 100);
    }
}

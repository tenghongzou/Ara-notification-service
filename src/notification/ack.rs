//! ACK (Acknowledgment) tracking for notifications
//!
//! This module provides functionality to track notification delivery confirmations
//! from clients. When a client receives a notification, it can send an ACK message
//! back to confirm receipt.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

use crate::metrics::{ACK_EXPIRED_TOTAL, ACK_LATENCY, ACK_RECEIVED_TOTAL, ACK_TRACKED_TOTAL};

/// Configuration for ACK tracking
#[derive(Debug, Clone)]
pub struct AckConfig {
    /// Whether ACK tracking is enabled
    pub enabled: bool,
    /// Timeout in seconds after which pending ACKs are considered expired
    pub timeout_seconds: u64,
    /// Interval in seconds for cleanup task
    pub cleanup_interval_seconds: u64,
}

impl Default for AckConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_seconds: 30,
            cleanup_interval_seconds: 60,
        }
    }
}

/// Information about a pending ACK
#[derive(Debug, Clone)]
pub struct PendingAck {
    /// The notification ID
    pub notification_id: Uuid,
    /// The user ID who should acknowledge
    pub user_id: String,
    /// Timestamp when the notification was sent
    pub sent_at: DateTime<Utc>,
    /// Connection ID that received the notification
    pub connection_id: Uuid,
}

impl PendingAck {
    pub fn new(notification_id: Uuid, user_id: String, connection_id: Uuid) -> Self {
        Self {
            notification_id,
            user_id,
            sent_at: Utc::now(),
            connection_id,
        }
    }

    /// Check if this pending ACK has expired
    pub fn is_expired(&self, timeout_seconds: u64) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.sent_at);
        elapsed.num_seconds() >= timeout_seconds as i64
    }
}

/// Statistics for ACK tracking
#[derive(Debug, Default)]
pub struct AckStats {
    /// Total notifications tracked for ACK
    pub total_tracked: AtomicU64,
    /// Total ACKs received
    pub total_acked: AtomicU64,
    /// Total expired (unacknowledged) notifications
    pub total_expired: AtomicU64,
    /// Cumulative latency in milliseconds (for calculating average)
    total_latency_ms: AtomicU64,
}

/// Snapshot of ACK statistics
#[derive(Debug, Clone, Serialize)]
pub struct AckStatsSnapshot {
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

/// Tracks notification acknowledgments from clients
pub struct AckTracker {
    /// Configuration
    config: AckConfig,
    /// Pending ACKs: notification_id -> PendingAck
    pending: DashMap<Uuid, PendingAck>,
    /// Statistics
    stats: AckStats,
}

impl AckTracker {
    /// Create a new ACK tracker with default configuration (disabled)
    pub fn new() -> Self {
        Self::with_config(AckConfig::default())
    }

    /// Create a new ACK tracker with the given configuration
    pub fn with_config(config: AckConfig) -> Self {
        Self {
            config,
            pending: DashMap::new(),
            stats: AckStats::default(),
        }
    }

    /// Check if ACK tracking is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Track a notification for ACK
    /// Call this when a notification is sent to a connection
    pub fn track(&self, notification_id: Uuid, user_id: &str, connection_id: Uuid) {
        if !self.config.enabled {
            return;
        }

        let pending = PendingAck::new(notification_id, user_id.to_string(), connection_id);
        self.pending.insert(notification_id, pending);
        self.stats.total_tracked.fetch_add(1, Ordering::Relaxed);
        ACK_TRACKED_TOTAL.inc();

        tracing::trace!(
            notification_id = %notification_id,
            user_id = %user_id,
            connection_id = %connection_id,
            "Tracking notification for ACK"
        );
    }

    /// Acknowledge a notification
    /// Returns true if the ACK was valid (notification was pending), false otherwise
    pub fn acknowledge(&self, notification_id: Uuid, user_id: &str) -> bool {
        if !self.config.enabled {
            return false;
        }

        if let Some((_, pending)) = self.pending.remove(&notification_id) {
            // Verify user_id matches
            if pending.user_id != user_id {
                tracing::warn!(
                    notification_id = %notification_id,
                    expected_user = %pending.user_id,
                    actual_user = %user_id,
                    "ACK user mismatch"
                );
                // Re-insert since this ACK is invalid
                self.pending.insert(notification_id, pending);
                return false;
            }

            // Calculate latency
            let latency_ms = Utc::now()
                .signed_duration_since(pending.sent_at)
                .num_milliseconds()
                .max(0) as u64;

            self.stats.total_acked.fetch_add(1, Ordering::Relaxed);
            self.stats.total_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);
            ACK_RECEIVED_TOTAL.inc();
            ACK_LATENCY.observe(latency_ms as f64 / 1000.0);

            tracing::debug!(
                notification_id = %notification_id,
                user_id = %user_id,
                latency_ms = latency_ms,
                "Notification acknowledged"
            );

            true
        } else {
            tracing::debug!(
                notification_id = %notification_id,
                user_id = %user_id,
                "ACK received for unknown notification"
            );
            false
        }
    }

    /// Clean up expired pending ACKs
    /// Returns the number of expired ACKs removed
    pub fn cleanup_expired(&self) -> usize {
        if !self.config.enabled {
            return 0;
        }

        let mut expired_count = 0;

        self.pending.retain(|_, pending| {
            if pending.is_expired(self.config.timeout_seconds) {
                expired_count += 1;
                false
            } else {
                true
            }
        });

        if expired_count > 0 {
            self.stats.total_expired.fetch_add(expired_count as u64, Ordering::Relaxed);
            ACK_EXPIRED_TOTAL.inc_by(expired_count as u64);
            tracing::debug!(
                expired = expired_count,
                "Cleaned up expired pending ACKs"
            );
        }

        expired_count
    }

    /// Get the current pending ACK count
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Get ACK statistics snapshot
    pub fn stats(&self) -> AckStatsSnapshot {
        let total_tracked = self.stats.total_tracked.load(Ordering::Relaxed);
        let total_acked = self.stats.total_acked.load(Ordering::Relaxed);
        let total_expired = self.stats.total_expired.load(Ordering::Relaxed);
        let total_latency_ms = self.stats.total_latency_ms.load(Ordering::Relaxed);
        let pending_count = self.pending.len() as u64;

        // Calculate ACK rate (avoid division by zero)
        let completed = total_acked + total_expired;
        let ack_rate = if completed > 0 {
            total_acked as f64 / completed as f64
        } else {
            1.0 // No completions yet, assume 100%
        };

        // Calculate average latency
        let avg_latency_ms = if total_acked > 0 {
            total_latency_ms / total_acked
        } else {
            0
        };

        AckStatsSnapshot {
            total_tracked,
            total_acked,
            total_expired,
            pending_count,
            ack_rate,
            avg_latency_ms,
        }
    }

    /// Get the cleanup interval from config
    pub fn cleanup_interval_seconds(&self) -> u64 {
        self.config.cleanup_interval_seconds
    }
}

impl Default for AckTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_enabled_tracker() -> AckTracker {
        AckTracker::with_config(AckConfig {
            enabled: true,
            timeout_seconds: 1,
            cleanup_interval_seconds: 60,
        })
    }

    #[test]
    fn test_track_disabled() {
        let tracker = AckTracker::new();
        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        tracker.track(notif_id, "user-1", conn_id);

        // Should not track when disabled
        assert_eq!(tracker.pending_count(), 0);
    }

    #[test]
    fn test_track_enabled() {
        let tracker = create_enabled_tracker();
        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        tracker.track(notif_id, "user-1", conn_id);

        assert_eq!(tracker.pending_count(), 1);
        assert_eq!(tracker.stats().total_tracked, 1);
    }

    #[test]
    fn test_acknowledge_success() {
        let tracker = create_enabled_tracker();
        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        tracker.track(notif_id, "user-1", conn_id);
        assert!(tracker.acknowledge(notif_id, "user-1"));

        assert_eq!(tracker.pending_count(), 0);
        assert_eq!(tracker.stats().total_acked, 1);
    }

    #[test]
    fn test_acknowledge_wrong_user() {
        let tracker = create_enabled_tracker();
        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        tracker.track(notif_id, "user-1", conn_id);

        // Wrong user should fail
        assert!(!tracker.acknowledge(notif_id, "user-2"));

        // Should still be pending
        assert_eq!(tracker.pending_count(), 1);
        assert_eq!(tracker.stats().total_acked, 0);
    }

    #[test]
    fn test_acknowledge_unknown() {
        let tracker = create_enabled_tracker();
        let notif_id = Uuid::new_v4();

        // Acknowledge unknown notification
        assert!(!tracker.acknowledge(notif_id, "user-1"));
        assert_eq!(tracker.stats().total_acked, 0);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let tracker = AckTracker::with_config(AckConfig {
            enabled: true,
            timeout_seconds: 0, // Immediate expiry
            cleanup_interval_seconds: 60,
        });

        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        tracker.track(notif_id, "user-1", conn_id);

        // Wait a bit for expiry
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let expired = tracker.cleanup_expired();
        assert_eq!(expired, 1);
        assert_eq!(tracker.pending_count(), 0);
        assert_eq!(tracker.stats().total_expired, 1);
    }

    #[test]
    fn test_stats_ack_rate() {
        let tracker = create_enabled_tracker();

        // Track 3 notifications
        for _ in 0..3 {
            let notif_id = Uuid::new_v4();
            let conn_id = Uuid::new_v4();
            tracker.track(notif_id, "user-1", conn_id);
        }

        // Get all pending and acknowledge 2 of them
        let pending_ids: Vec<Uuid> = tracker.pending.iter().map(|r| *r.key()).collect();
        tracker.acknowledge(pending_ids[0], "user-1");
        tracker.acknowledge(pending_ids[1], "user-1");

        // Manually expire the third one
        tracker.stats.total_expired.fetch_add(1, Ordering::Relaxed);
        tracker.pending.remove(&pending_ids[2]);

        let stats = tracker.stats();
        assert_eq!(stats.total_acked, 2);
        assert_eq!(stats.total_expired, 1);
        // ACK rate should be 2/3 = 0.666...
        assert!((stats.ack_rate - 0.6666).abs() < 0.01);
    }

    #[test]
    fn test_stats_avg_latency() {
        let tracker = create_enabled_tracker();
        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        tracker.track(notif_id, "user-1", conn_id);

        // Immediate ACK should have very low latency
        tracker.acknowledge(notif_id, "user-1");

        let stats = tracker.stats();
        // Latency should be very low (< 100ms for immediate ACK)
        assert!(stats.avg_latency_ms < 100);
    }

    #[test]
    fn test_multiple_users() {
        let tracker = create_enabled_tracker();

        let notif1 = Uuid::new_v4();
        let notif2 = Uuid::new_v4();
        let conn1 = Uuid::new_v4();
        let conn2 = Uuid::new_v4();

        tracker.track(notif1, "user-1", conn1);
        tracker.track(notif2, "user-2", conn2);

        assert_eq!(tracker.pending_count(), 2);

        // Each user can only ACK their own notification
        assert!(tracker.acknowledge(notif1, "user-1"));
        assert!(!tracker.acknowledge(notif2, "user-1")); // Wrong user
        assert!(tracker.acknowledge(notif2, "user-2"));

        assert_eq!(tracker.pending_count(), 0);
        assert_eq!(tracker.stats().total_acked, 2);
    }
}

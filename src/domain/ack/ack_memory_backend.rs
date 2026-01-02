//! In-memory ACK tracking backend using DashMap.
//!
//! This module provides a memory-based implementation of the `AckTrackerBackend` trait.
//! ACK tracking state is stored in memory and will be lost on service restart.

use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use dashmap::DashMap;
use uuid::Uuid;

use crate::metrics::{ACK_EXPIRED_TOTAL, ACK_LATENCY, ACK_RECEIVED_TOTAL, ACK_TRACKED_TOTAL};
use crate::notification::ack::AckConfig;

use super::ack_backend::{AckBackendError, AckBackendStats, AckTrackerBackend, PendingAckInfo};

/// Statistics for ACK tracking (atomic counters for thread safety).
#[derive(Debug, Default)]
struct AckStats {
    /// Total notifications tracked for ACK
    total_tracked: AtomicU64,
    /// Total ACKs received
    total_acked: AtomicU64,
    /// Total expired (unacknowledged) notifications
    total_expired: AtomicU64,
    /// Cumulative latency in milliseconds (for calculating average)
    total_latency_ms: AtomicU64,
}

/// In-memory ACK tracking backend.
///
/// Uses `DashMap` for concurrent access to pending ACKs.
pub struct MemoryAckBackend {
    /// Configuration
    config: AckConfig,
    /// Pending ACKs: notification_id -> PendingAckInfo
    pending: DashMap<Uuid, PendingAckInfo>,
    /// Statistics
    stats: AckStats,
}

impl MemoryAckBackend {
    /// Create a new memory ACK backend with the given configuration.
    pub fn new(config: AckConfig) -> Self {
        Self {
            config,
            pending: DashMap::new(),
            stats: AckStats::default(),
        }
    }
}

#[async_trait]
impl AckTrackerBackend for MemoryAckBackend {
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

    async fn acknowledge(&self, notification_id: Uuid, user_id: &str) -> bool {
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
            let latency_ms = pending.latency_ms();

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

    async fn get_pending(&self, notification_id: Uuid) -> Result<Option<PendingAckInfo>, AckBackendError> {
        Ok(self.pending.get(&notification_id).map(|r| r.value().clone()))
    }

    async fn cleanup_expired(&self) -> usize {
        if !self.config.enabled {
            return 0;
        }

        let timeout = self.config.timeout_seconds;
        let mut expired_count = 0;

        self.pending.retain(|_, pending| {
            if pending.is_expired(timeout) {
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

    async fn pending_count(&self) -> usize {
        self.pending.len()
    }

    async fn stats(&self) -> AckBackendStats {
        let total_tracked = self.stats.total_tracked.load(Ordering::Relaxed);
        let total_acked = self.stats.total_acked.load(Ordering::Relaxed);
        let total_expired = self.stats.total_expired.load(Ordering::Relaxed);
        let total_latency_ms = self.stats.total_latency_ms.load(Ordering::Relaxed);
        let pending_count = self.pending.len() as u64;

        AckBackendStats {
            backend_type: "memory".to_string(),
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

    fn create_enabled_config() -> AckConfig {
        AckConfig {
            enabled: true,
            timeout_seconds: 30,
            cleanup_interval_seconds: 60,
        }
    }

    #[tokio::test]
    async fn test_track_disabled() {
        let config = AckConfig {
            enabled: false,
            ..Default::default()
        };
        let backend = MemoryAckBackend::new(config);

        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        backend.track(notif_id, "user-1", conn_id).await;

        // Should not track when disabled
        assert_eq!(backend.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_track_enabled() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        backend.track(notif_id, "user-1", conn_id).await;

        assert_eq!(backend.pending_count().await, 1);
        assert_eq!(backend.stats().await.total_tracked, 1);
    }

    #[tokio::test]
    async fn test_acknowledge_success() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        backend.track(notif_id, "user-1", conn_id).await;
        assert!(backend.acknowledge(notif_id, "user-1").await);

        assert_eq!(backend.pending_count().await, 0);
        assert_eq!(backend.stats().await.total_acked, 1);
    }

    #[tokio::test]
    async fn test_acknowledge_wrong_user() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        backend.track(notif_id, "user-1", conn_id).await;

        // Wrong user should fail
        assert!(!backend.acknowledge(notif_id, "user-2").await);

        // Should still be pending
        assert_eq!(backend.pending_count().await, 1);
        assert_eq!(backend.stats().await.total_acked, 0);
    }

    #[tokio::test]
    async fn test_acknowledge_unknown() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        let notif_id = Uuid::new_v4();

        // Acknowledge unknown notification
        assert!(!backend.acknowledge(notif_id, "user-1").await);
        assert_eq!(backend.stats().await.total_acked, 0);
    }

    #[tokio::test]
    async fn test_get_pending() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        backend.track(notif_id, "user-1", conn_id).await;

        let pending = backend.get_pending(notif_id).await.unwrap();
        assert!(pending.is_some());

        let info = pending.unwrap();
        assert_eq!(info.notification_id, notif_id);
        assert_eq!(info.user_id, "user-1");
        assert_eq!(info.connection_id, conn_id);
    }

    #[tokio::test]
    async fn test_get_pending_not_found() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        let notif_id = Uuid::new_v4();

        let pending = backend.get_pending(notif_id).await.unwrap();
        assert!(pending.is_none());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let config = AckConfig {
            enabled: true,
            timeout_seconds: 0, // Immediate expiry
            cleanup_interval_seconds: 60,
        };
        let backend = MemoryAckBackend::new(config);

        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        backend.track(notif_id, "user-1", conn_id).await;

        // Wait a bit for expiry
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let expired = backend.cleanup_expired().await;
        assert_eq!(expired, 1);
        assert_eq!(backend.pending_count().await, 0);
        assert_eq!(backend.stats().await.total_expired, 1);
    }

    #[tokio::test]
    async fn test_stats_ack_rate() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        // Track 3 notifications
        let mut notif_ids = Vec::new();
        for _ in 0..3 {
            let notif_id = Uuid::new_v4();
            let conn_id = Uuid::new_v4();
            backend.track(notif_id, "user-1", conn_id).await;
            notif_ids.push(notif_id);
        }

        // Acknowledge 2 of them
        backend.acknowledge(notif_ids[0], "user-1").await;
        backend.acknowledge(notif_ids[1], "user-1").await;

        // Manually mark the third as expired
        backend.pending.remove(&notif_ids[2]);
        backend.stats.total_expired.fetch_add(1, Ordering::Relaxed);

        let stats = backend.stats().await;
        assert_eq!(stats.total_acked, 2);
        assert_eq!(stats.total_expired, 1);
        // ACK rate should be 2/3 = 0.666...
        assert!((stats.ack_rate - 0.6666).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_stats_avg_latency() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        let notif_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();

        backend.track(notif_id, "user-1", conn_id).await;

        // Immediate ACK should have very low latency
        backend.acknowledge(notif_id, "user-1").await;

        let stats = backend.stats().await;
        // Latency should be very low (< 100ms for immediate ACK)
        assert!(stats.avg_latency_ms < 100);
    }

    #[tokio::test]
    async fn test_multiple_users() {
        let backend = MemoryAckBackend::new(create_enabled_config());

        let notif1 = Uuid::new_v4();
        let notif2 = Uuid::new_v4();
        let conn1 = Uuid::new_v4();
        let conn2 = Uuid::new_v4();

        backend.track(notif1, "user-1", conn1).await;
        backend.track(notif2, "user-2", conn2).await;

        assert_eq!(backend.pending_count().await, 2);

        // Each user can only ACK their own notification
        assert!(backend.acknowledge(notif1, "user-1").await);
        assert!(!backend.acknowledge(notif2, "user-1").await); // Wrong user
        assert!(backend.acknowledge(notif2, "user-2").await);

        assert_eq!(backend.pending_count().await, 0);
        assert_eq!(backend.stats().await.total_acked, 2);
    }

    #[tokio::test]
    async fn test_stats_backend_type() {
        let backend = MemoryAckBackend::new(create_enabled_config());
        let stats = backend.stats().await;
        assert_eq!(stats.backend_type, "memory");
    }
}

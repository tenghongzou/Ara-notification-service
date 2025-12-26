use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use uuid::Uuid;

use crate::connection_manager::ConnectionManager;
use crate::websocket::ServerMessage;

use super::{NotificationEvent, NotificationTarget};

/// Result of a notification delivery attempt
#[derive(Debug, Clone, Serialize)]
pub struct DeliveryResult {
    /// Notification ID
    pub notification_id: Uuid,
    /// Number of connections the message was delivered to
    pub delivered_to: usize,
    /// Number of connections that failed to receive
    pub failed: usize,
    /// Whether any delivery was successful
    pub success: bool,
}

impl DeliveryResult {
    fn new(notification_id: Uuid, delivered: usize, failed: usize) -> Self {
        Self {
            notification_id,
            delivered_to: delivered,
            failed,
            success: delivered > 0,
        }
    }
}

/// Statistics for the notification dispatcher
#[derive(Debug, Default)]
pub struct DispatcherStats {
    /// Total notifications sent
    pub total_sent: AtomicU64,
    /// Total successful deliveries (connection count)
    pub total_delivered: AtomicU64,
    /// Total failed deliveries
    pub total_failed: AtomicU64,
    /// Point-to-point notifications
    pub user_notifications: AtomicU64,
    /// Broadcast notifications
    pub broadcast_notifications: AtomicU64,
    /// Channel notifications
    pub channel_notifications: AtomicU64,
}

impl DispatcherStats {
    pub fn snapshot(&self) -> DispatcherStatsSnapshot {
        DispatcherStatsSnapshot {
            total_sent: self.total_sent.load(Ordering::Relaxed),
            total_delivered: self.total_delivered.load(Ordering::Relaxed),
            total_failed: self.total_failed.load(Ordering::Relaxed),
            user_notifications: self.user_notifications.load(Ordering::Relaxed),
            broadcast_notifications: self.broadcast_notifications.load(Ordering::Relaxed),
            channel_notifications: self.channel_notifications.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of dispatcher statistics
#[derive(Debug, Clone, Serialize)]
pub struct DispatcherStatsSnapshot {
    pub total_sent: u64,
    pub total_delivered: u64,
    pub total_failed: u64,
    pub user_notifications: u64,
    pub broadcast_notifications: u64,
    pub channel_notifications: u64,
}

/// Dispatches notifications to connected clients
pub struct NotificationDispatcher {
    connection_manager: Arc<ConnectionManager>,
    stats: DispatcherStats,
}

impl NotificationDispatcher {
    /// Create a new dispatcher
    pub fn new(connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            connection_manager,
            stats: DispatcherStats::default(),
        }
    }

    /// Get dispatcher statistics
    pub fn stats(&self) -> DispatcherStatsSnapshot {
        self.stats.snapshot()
    }

    /// Dispatch a notification to the specified target
    pub async fn dispatch(&self, target: NotificationTarget, event: NotificationEvent) -> DeliveryResult {
        // Skip expired notifications
        if event.is_expired() {
            tracing::debug!(
                notification_id = %event.id,
                "Skipping expired notification"
            );
            return DeliveryResult::new(event.id, 0, 0);
        }

        match target {
            NotificationTarget::User(user_id) => self.send_to_user(&user_id, event).await,
            NotificationTarget::Users(user_ids) => self.send_to_users(&user_ids, event).await,
            NotificationTarget::Broadcast => self.broadcast(event).await,
            NotificationTarget::Channel(channel) => self.send_to_channel(&channel, event).await,
            NotificationTarget::Channels(channels) => self.send_to_channels(&channels, event).await,
        }
    }

    /// Send notification to a specific user (all their connections)
    pub async fn send_to_user(&self, user_id: &str, event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let connections = self.connection_manager.get_user_connections(user_id);
        let message = ServerMessage::Notification { event };

        let (delivered, failed) = self.send_to_connections(&connections, &message).await;

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.stats.user_notifications.fetch_add(1, Ordering::Relaxed);

        tracing::debug!(
            user_id = %user_id,
            notification_id = %notification_id,
            delivered = delivered,
            failed = failed,
            "Sent notification to user"
        );

        DeliveryResult::new(notification_id, delivered, failed)
    }

    /// Send notification to multiple users
    pub async fn send_to_users(&self, user_ids: &[String], event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let message = ServerMessage::Notification { event };

        let mut total_delivered = 0;
        let mut total_failed = 0;

        for user_id in user_ids {
            let connections = self.connection_manager.get_user_connections(user_id);
            let (delivered, failed) = self.send_to_connections(&connections, &message).await;
            total_delivered += delivered;
            total_failed += failed;
        }

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(total_delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(total_failed as u64, Ordering::Relaxed);
        self.stats.user_notifications.fetch_add(1, Ordering::Relaxed);

        tracing::debug!(
            user_count = user_ids.len(),
            notification_id = %notification_id,
            delivered = total_delivered,
            failed = total_failed,
            "Sent notification to multiple users"
        );

        DeliveryResult::new(notification_id, total_delivered, total_failed)
    }

    /// Broadcast notification to all connected users
    pub async fn broadcast(&self, event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let connections = self.connection_manager.get_all_connections();
        let message = ServerMessage::Notification { event };

        let (delivered, failed) = self.send_to_connections(&connections, &message).await;

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.stats.broadcast_notifications.fetch_add(1, Ordering::Relaxed);

        tracing::debug!(
            notification_id = %notification_id,
            delivered = delivered,
            failed = failed,
            "Broadcast notification to all connections"
        );

        DeliveryResult::new(notification_id, delivered, failed)
    }

    /// Send notification to a specific channel
    pub async fn send_to_channel(&self, channel: &str, event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let connections = self.connection_manager.get_channel_connections(channel);
        let message = ServerMessage::Notification { event };

        let (delivered, failed) = self.send_to_connections(&connections, &message).await;

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.stats.channel_notifications.fetch_add(1, Ordering::Relaxed);

        tracing::debug!(
            channel = %channel,
            notification_id = %notification_id,
            delivered = delivered,
            failed = failed,
            "Sent notification to channel"
        );

        DeliveryResult::new(notification_id, delivered, failed)
    }

    /// Send notification to multiple channels
    pub async fn send_to_channels(&self, channels: &[String], event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let message = ServerMessage::Notification { event };

        // Collect unique connections from all channels
        let mut seen_connections = std::collections::HashSet::new();
        let mut all_connections = Vec::new();

        for channel in channels {
            for conn in self.connection_manager.get_channel_connections(channel) {
                if seen_connections.insert(conn.id) {
                    all_connections.push(conn);
                }
            }
        }

        let (delivered, failed) = self.send_to_connections(&all_connections, &message).await;

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.stats.channel_notifications.fetch_add(1, Ordering::Relaxed);

        tracing::debug!(
            channels = ?channels,
            notification_id = %notification_id,
            delivered = delivered,
            failed = failed,
            "Sent notification to multiple channels"
        );

        DeliveryResult::new(notification_id, delivered, failed)
    }

    /// Send message to a list of connections
    async fn send_to_connections(
        &self,
        connections: &[Arc<crate::connection_manager::ConnectionHandle>],
        message: &ServerMessage,
    ) -> (usize, usize) {
        let mut delivered = 0;
        let mut failed = 0;

        for conn in connections {
            match conn.send(message.clone()).await {
                Ok(_) => delivered += 1,
                Err(_) => failed += 1,
            }
        }

        (delivered, failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delivery_result() {
        let result = DeliveryResult::new(Uuid::new_v4(), 5, 2);
        assert!(result.success);
        assert_eq!(result.delivered_to, 5);
        assert_eq!(result.failed, 2);

        let empty_result = DeliveryResult::new(Uuid::new_v4(), 0, 0);
        assert!(!empty_result.success);
    }

    #[test]
    fn test_stats_snapshot() {
        let stats = DispatcherStats::default();
        stats.total_sent.fetch_add(10, Ordering::Relaxed);
        stats.total_delivered.fetch_add(25, Ordering::Relaxed);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_sent, 10);
        assert_eq!(snapshot.total_delivered, 25);
    }
}

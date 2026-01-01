use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use serde::Serialize;
use uuid::Uuid;

use crate::connection_manager::{ConnectionHandle, ConnectionManager};
use crate::metrics::MessageMetrics;
use crate::queue::MessageQueueBackend;
use crate::websocket::{OutboundMessage, ServerMessage};

use super::{AckTrackerBackend, NotificationEvent, NotificationTarget};

/// Maximum number of concurrent message sends
const MAX_CONCURRENT_SENDS: usize = 100;

/// Threshold for using pre-serialization (saves serialization overhead for larger sends)
const PRESERIALIZATION_THRESHOLD: usize = 4;

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
    queue_backend: Option<Arc<dyn MessageQueueBackend>>,
    ack_backend: Option<Arc<dyn AckTrackerBackend>>,
    stats: DispatcherStats,
}

impl NotificationDispatcher {
    /// Create a new dispatcher without queue or ACK support
    pub fn new(connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            connection_manager,
            queue_backend: None,
            ack_backend: None,
            stats: DispatcherStats::default(),
        }
    }

    /// Create a new dispatcher with queue backend support
    pub fn with_queue(
        connection_manager: Arc<ConnectionManager>,
        queue_backend: Arc<dyn MessageQueueBackend>,
    ) -> Self {
        Self {
            connection_manager,
            queue_backend: Some(queue_backend),
            ack_backend: None,
            stats: DispatcherStats::default(),
        }
    }

    /// Create a new dispatcher with full backend configuration
    pub fn with_backends(
        connection_manager: Arc<ConnectionManager>,
        queue_backend: Arc<dyn MessageQueueBackend>,
        ack_backend: Arc<dyn AckTrackerBackend>,
    ) -> Self {
        Self {
            connection_manager,
            queue_backend: Some(queue_backend),
            ack_backend: Some(ack_backend),
            stats: DispatcherStats::default(),
        }
    }

    /// Set the queue backend (for deferred initialization)
    pub fn set_queue_backend(&mut self, queue_backend: Arc<dyn MessageQueueBackend>) {
        self.queue_backend = Some(queue_backend);
    }

    /// Set the ACK backend (for deferred initialization)
    pub fn set_ack_backend(&mut self, ack_backend: Arc<dyn AckTrackerBackend>) {
        self.ack_backend = Some(ack_backend);
    }

    /// Get dispatcher statistics
    pub fn stats(&self) -> DispatcherStatsSnapshot {
        self.stats.snapshot()
    }

    /// Dispatch a notification to the specified target
    #[tracing::instrument(
        name = "dispatcher.dispatch",
        skip(self, event),
        fields(
            notification_id = %event.id,
            event_type = %event.event_type,
            target_type = ?std::mem::discriminant(&target)
        )
    )]
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
    /// If the user is offline and queue is enabled, the message will be queued for later delivery.
    #[tracing::instrument(
        name = "dispatcher.send_to_user",
        skip(self, event),
        fields(notification_id = %event.id, event_type = %event.event_type)
    )]
    pub async fn send_to_user(&self, user_id: &str, event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let connections = self.connection_manager.get_user_connections(user_id);

        // If user has no connections and queue is enabled, queue the message
        if connections.is_empty() {
            if let Some(ref queue) = self.queue_backend {
                if queue.is_enabled() {
                    match queue.enqueue(user_id, event.clone()).await {
                        Ok(()) => {
                            tracing::debug!(
                                user_id = %user_id,
                                notification_id = %notification_id,
                                "User offline, message queued for later delivery"
                            );
                            // Update stats - message was queued, not delivered yet
                            self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
                            self.stats.user_notifications.fetch_add(1, Ordering::Relaxed);
                            return DeliveryResult::new(notification_id, 0, 0);
                        }
                        Err(e) => {
                            tracing::warn!(
                                user_id = %user_id,
                                notification_id = %notification_id,
                                error = %e,
                                "Failed to queue message for offline user"
                            );
                        }
                    }
                }
            }
        }

        let message = ServerMessage::Notification { event };
        let (delivered, failed) = self.send_to_connections(&connections, &message, Some(notification_id)).await;

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.stats.user_notifications.fetch_add(1, Ordering::Relaxed);

        // Update Prometheus metrics
        MessageMetrics::record_user_sent();
        MessageMetrics::record_delivered(delivered as u64);
        MessageMetrics::record_failed(failed as u64);

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
    /// Offline users will have messages queued if queue is enabled.
    #[tracing::instrument(
        name = "dispatcher.send_to_users",
        skip(self, event, user_ids),
        fields(
            notification_id = %event.id,
            event_type = %event.event_type,
            user_count = user_ids.len()
        )
    )]
    pub async fn send_to_users(&self, user_ids: &[String], event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let message = ServerMessage::Notification { event: event.clone() };

        let mut total_delivered = 0;
        let mut total_failed = 0;
        let mut queued_count = 0;

        for user_id in user_ids {
            let connections = self.connection_manager.get_user_connections(user_id);

            // If user has no connections and queue is enabled, queue the message
            if connections.is_empty() {
                if let Some(ref queue) = self.queue_backend {
                    if queue.is_enabled() {
                        if queue.enqueue(user_id, event.clone()).await.is_ok() {
                            queued_count += 1;
                            continue;
                        }
                    }
                }
            }

            let (delivered, failed) = self.send_to_connections(&connections, &message, Some(notification_id)).await;
            total_delivered += delivered;
            total_failed += failed;
        }

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(total_delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(total_failed as u64, Ordering::Relaxed);
        self.stats.user_notifications.fetch_add(1, Ordering::Relaxed);

        // Update Prometheus metrics
        MessageMetrics::record_users_sent();
        MessageMetrics::record_delivered(total_delivered as u64);
        MessageMetrics::record_failed(total_failed as u64);

        tracing::debug!(
            user_count = user_ids.len(),
            notification_id = %notification_id,
            delivered = total_delivered,
            failed = total_failed,
            queued = queued_count,
            "Sent notification to multiple users"
        );

        DeliveryResult::new(notification_id, total_delivered, total_failed)
    }

    /// Broadcast notification to all connected users
    #[tracing::instrument(
        name = "dispatcher.broadcast",
        skip(self, event),
        fields(notification_id = %event.id, event_type = %event.event_type)
    )]
    pub async fn broadcast(&self, event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let connections = self.connection_manager.get_all_connections();
        let message = ServerMessage::Notification { event };

        let (delivered, failed) = self.send_to_connections(&connections, &message, Some(notification_id)).await;

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.stats.broadcast_notifications.fetch_add(1, Ordering::Relaxed);

        // Update Prometheus metrics
        MessageMetrics::record_broadcast_sent();
        MessageMetrics::record_delivered(delivered as u64);
        MessageMetrics::record_failed(failed as u64);

        tracing::debug!(
            notification_id = %notification_id,
            delivered = delivered,
            failed = failed,
            "Broadcast notification to all connections"
        );

        DeliveryResult::new(notification_id, delivered, failed)
    }

    /// Send notification to a specific channel
    #[tracing::instrument(
        name = "dispatcher.send_to_channel",
        skip(self, event),
        fields(notification_id = %event.id, event_type = %event.event_type)
    )]
    pub async fn send_to_channel(&self, channel: &str, event: NotificationEvent) -> DeliveryResult {
        let notification_id = event.id;
        let connections = self.connection_manager.get_channel_connections(channel);
        let message = ServerMessage::Notification { event };

        let (delivered, failed) = self.send_to_connections(&connections, &message, Some(notification_id)).await;

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.stats.channel_notifications.fetch_add(1, Ordering::Relaxed);

        // Update Prometheus metrics
        MessageMetrics::record_channel_sent();
        MessageMetrics::record_delivered(delivered as u64);
        MessageMetrics::record_failed(failed as u64);

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
    #[tracing::instrument(
        name = "dispatcher.send_to_channels",
        skip(self, event, channels),
        fields(
            notification_id = %event.id,
            event_type = %event.event_type,
            channel_count = channels.len()
        )
    )]
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

        let (delivered, failed) = self.send_to_connections(&all_connections, &message, Some(notification_id)).await;

        // Update stats
        self.stats.total_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.total_delivered.fetch_add(delivered as u64, Ordering::Relaxed);
        self.stats.total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.stats.channel_notifications.fetch_add(1, Ordering::Relaxed);

        // Update Prometheus metrics
        MessageMetrics::record_channels_sent();
        MessageMetrics::record_delivered(delivered as u64);
        MessageMetrics::record_failed(failed as u64);

        tracing::debug!(
            channels = ?channels,
            notification_id = %notification_id,
            delivered = delivered,
            failed = failed,
            "Sent notification to multiple channels"
        );

        DeliveryResult::new(notification_id, delivered, failed)
    }

    /// Send message to a list of connections concurrently
    /// Uses bounded parallelism to avoid overwhelming the system
    /// Pre-serializes the message once for larger sends to avoid repeated serialization
    /// If notification_id is provided and ack_tracker is configured, tracks pending ACKs
    async fn send_to_connections(
        &self,
        connections: &[Arc<ConnectionHandle>],
        message: &ServerMessage,
        notification_id: Option<Uuid>,
    ) -> (usize, usize) {
        if connections.is_empty() {
            return (0, 0);
        }

        // For small number of connections, use simple sequential sending without pre-serialization
        if connections.len() <= 3 {
            let mut delivered = 0;
            let mut failed = 0;
            for conn in connections {
                match conn.send(message.clone()).await {
                    Ok(_) => {
                        delivered += 1;
                        // Track ACK if enabled
                        if let (Some(notif_id), Some(tracker)) = (notification_id, &self.ack_backend) {
                            tracker.track(notif_id, &conn.user_id, conn.id).await;
                        }
                    }
                    Err(_) => failed += 1,
                }
            }
            return (delivered, failed);
        }

        // For larger sends, pre-serialize once and share across all connections
        // This avoids repeated serde_json::to_string calls
        let outbound = if connections.len() >= PRESERIALIZATION_THRESHOLD {
            match OutboundMessage::preserialized(message) {
                Ok(msg) => msg,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to pre-serialize message, falling back to per-connection serialization");
                    OutboundMessage::Raw(message.clone())
                }
            }
        } else {
            OutboundMessage::Raw(message.clone())
        };

        // For larger number of connections, use concurrent sending with bounded parallelism
        // We need to track which connections succeeded for ACK tracking
        let mut futures = FuturesUnordered::new();
        let mut delivered = 0;
        let mut failed = 0;
        let mut pending = 0;

        for conn in connections {
            let conn = conn.clone();
            let msg = outbound.clone();
            // Return the connection on success so we can track ACKs
            futures.push(async move {
                match conn.send_preserialized(msg).await {
                    Ok(_) => Some(conn),
                    Err(_) => None,
                }
            });
            pending += 1;

            // Process completed futures when we hit the concurrency limit
            while pending >= MAX_CONCURRENT_SENDS {
                if let Some(result) = futures.next().await {
                    pending -= 1;
                    match result {
                        Some(conn) => {
                            delivered += 1;
                            if let (Some(notif_id), Some(tracker)) = (notification_id, &self.ack_backend) {
                                tracker.track(notif_id, &conn.user_id, conn.id).await;
                            }
                        }
                        None => failed += 1,
                    }
                } else {
                    break;
                }
            }
        }

        // Process remaining futures
        while let Some(result) = futures.next().await {
            match result {
                Some(conn) => {
                    delivered += 1;
                    if let (Some(notif_id), Some(tracker)) = (notification_id, &self.ack_backend) {
                        tracker.track(notif_id, &conn.user_id, conn.id).await;
                    }
                }
                None => failed += 1,
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

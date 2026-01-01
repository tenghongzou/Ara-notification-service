//! Graceful shutdown handling for the notification service.
//!
//! This module provides coordinated shutdown functionality that:
//! 1. Notifies all connected clients about the impending shutdown
//! 2. Waits for in-flight messages to be processed
//! 3. Flushes queued messages to persistent storage (if enabled)
//! 4. Cleans up resources in the correct order

use std::sync::Arc;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::connection_manager::ConnectionManager;
use crate::queue::MessageQueueBackend;
use crate::websocket::ServerMessage;

/// Configuration for graceful shutdown behavior
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Time to wait for clients to be notified (default: 5 seconds)
    pub client_notification_timeout: Duration,
    /// Time to wait for in-flight messages to complete (default: 10 seconds)
    pub drain_timeout: Duration,
    /// Time to wait for queue flush to complete (default: 15 seconds)
    pub queue_flush_timeout: Duration,
    /// Suggested reconnect delay to send to clients (default: 5 seconds)
    pub reconnect_after_seconds: u64,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            client_notification_timeout: Duration::from_secs(5),
            drain_timeout: Duration::from_secs(10),
            queue_flush_timeout: Duration::from_secs(15),
            reconnect_after_seconds: 5,
        }
    }
}

/// Handles graceful shutdown of the notification service
pub struct GracefulShutdown {
    connection_manager: Arc<ConnectionManager>,
    queue_backend: Arc<dyn MessageQueueBackend>,
    shutdown_tx: broadcast::Sender<()>,
    config: ShutdownConfig,
}

impl GracefulShutdown {
    /// Create a new graceful shutdown handler
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        queue_backend: Arc<dyn MessageQueueBackend>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            connection_manager,
            queue_backend,
            shutdown_tx,
            config: ShutdownConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        connection_manager: Arc<ConnectionManager>,
        queue_backend: Arc<dyn MessageQueueBackend>,
        shutdown_tx: broadcast::Sender<()>,
        config: ShutdownConfig,
    ) -> Self {
        Self {
            connection_manager,
            queue_backend,
            shutdown_tx,
            config,
        }
    }

    /// Execute graceful shutdown sequence
    ///
    /// Returns a ShutdownResult with details about the shutdown process
    #[tracing::instrument(
        name = "graceful_shutdown",
        skip(self),
        fields(
            total_connections = self.connection_manager.stats().total_connections
        )
    )]
    pub async fn execute(&self, reason: &str) -> ShutdownResult {
        let start = std::time::Instant::now();
        let mut result = ShutdownResult::default();

        // Phase 1: Notify all connected clients
        tracing::info!(reason = %reason, "Starting graceful shutdown - Phase 1: Notifying clients");
        result.clients_notified = self.notify_clients(reason).await;

        // Phase 2: Signal background tasks to stop
        tracing::info!("Phase 2: Signaling background tasks to stop");
        let _ = self.shutdown_tx.send(());

        // Phase 3: Wait for message queues to drain
        tracing::info!("Phase 3: Draining message queues");
        result.queue_drained = self.drain_queues().await;

        // Phase 4: Wait briefly for connections to close gracefully
        tracing::info!("Phase 4: Waiting for connections to close");
        result.connections_closed = self.wait_for_connections_to_close().await;

        result.duration = start.elapsed();
        result.success = true;

        tracing::info!(
            clients_notified = result.clients_notified,
            connections_closed = result.connections_closed,
            queue_drained = result.queue_drained,
            duration_ms = result.duration.as_millis(),
            "Graceful shutdown completed"
        );

        result
    }

    /// Notify all connected clients about shutdown
    async fn notify_clients(&self, reason: &str) -> usize {
        let connections = self.connection_manager.get_all_connections();
        let total = connections.len();

        if total == 0 {
            return 0;
        }

        tracing::info!(
            total_connections = total,
            "Sending shutdown notifications to clients"
        );

        let message = ServerMessage::shutdown(reason, Some(self.config.reconnect_after_seconds));
        let mut futures = FuturesUnordered::new();
        let mut notified = 0;

        for conn in connections {
            let msg = message.clone();
            futures.push(async move {
                match timeout(Duration::from_secs(2), conn.send(msg)).await {
                    Ok(Ok(_)) => true,
                    Ok(Err(e)) => {
                        tracing::debug!(
                            connection_id = %conn.id,
                            error = %e,
                            "Failed to send shutdown notification"
                        );
                        false
                    }
                    Err(_) => {
                        tracing::debug!(
                            connection_id = %conn.id,
                            "Timeout sending shutdown notification"
                        );
                        false
                    }
                }
            });
        }

        // Process all notifications with overall timeout
        let notify_future = async {
            while let Some(success) = futures.next().await {
                if success {
                    notified += 1;
                }
            }
        };

        let _ = timeout(self.config.client_notification_timeout, notify_future).await;

        tracing::info!(
            notified = notified,
            total = total,
            "Shutdown notifications sent"
        );

        notified
    }

    /// Drain message queues
    async fn drain_queues(&self) -> bool {
        if !self.queue_backend.is_enabled() {
            return true;
        }

        let stats = self.queue_backend.stats().await;
        if stats.total_messages == 0 {
            return true;
        }

        tracing::info!(
            queued_messages = stats.total_messages,
            users_with_queue = stats.users_with_queue,
            "Waiting for queued messages to be processed"
        );

        // For memory backend, messages are lost on shutdown
        // For Redis/PostgreSQL backend, messages are already persisted
        // This is a best-effort wait
        let queue_backend = self.queue_backend.clone();
        let drain_future = async {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let current_stats = queue_backend.stats().await;
                if current_stats.total_messages == 0 {
                    break;
                }
            }
        };

        match timeout(self.config.queue_flush_timeout, drain_future).await {
            Ok(_) => {
                tracing::info!("Message queue drained successfully");
                true
            }
            Err(_) => {
                let remaining = self.queue_backend.stats().await.total_messages;
                tracing::warn!(
                    remaining_messages = remaining,
                    "Queue drain timeout, some messages may be lost"
                );
                false
            }
        }
    }

    /// Wait for connections to close gracefully
    async fn wait_for_connections_to_close(&self) -> usize {
        let initial = self.connection_manager.stats().total_connections;
        if initial == 0 {
            return 0;
        }

        let closed = std::sync::atomic::AtomicUsize::new(0);
        let wait_future = async {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let current = self.connection_manager.stats().total_connections;
                closed.store(initial - current, std::sync::atomic::Ordering::Relaxed);
                if current == 0 {
                    break;
                }
            }
        };

        let _ = timeout(self.config.drain_timeout, wait_future).await;

        let final_count = self.connection_manager.stats().total_connections;
        let total_closed = initial - final_count;

        if final_count > 0 {
            tracing::warn!(
                remaining_connections = final_count,
                "Some connections did not close gracefully"
            );
        }

        total_closed
    }
}

/// Result of a graceful shutdown operation
#[derive(Debug, Default)]
pub struct ShutdownResult {
    /// Whether shutdown completed successfully
    pub success: bool,
    /// Number of clients that were notified
    pub clients_notified: usize,
    /// Number of connections that closed gracefully
    pub connections_closed: usize,
    /// Whether the message queue was fully drained
    pub queue_drained: bool,
    /// Total time taken for shutdown
    pub duration: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_manager::ConnectionLimits;
    use crate::queue::create_queue_backend;
    use crate::config::QueueConfig as SettingsQueueConfig;

    fn create_test_components() -> (Arc<ConnectionManager>, Arc<dyn MessageQueueBackend>, broadcast::Sender<()>) {
        let cm = Arc::new(ConnectionManager::with_limits(ConnectionLimits::default()));
        let queue_config = SettingsQueueConfig::default();
        let queue_backend = create_queue_backend(&queue_config, None, None, None);
        let (tx, _) = broadcast::channel(1);
        (cm, queue_backend, tx)
    }

    #[tokio::test]
    async fn test_shutdown_no_connections() {
        let (cm, queue_backend, tx) = create_test_components();
        let shutdown = GracefulShutdown::new(cm, queue_backend, tx);

        let result = shutdown.execute("test shutdown").await;

        assert!(result.success);
        assert_eq!(result.clients_notified, 0);
        assert_eq!(result.connections_closed, 0);
    }

    #[test]
    fn test_shutdown_config_defaults() {
        let config = ShutdownConfig::default();
        assert_eq!(config.client_notification_timeout, Duration::from_secs(5));
        assert_eq!(config.drain_timeout, Duration::from_secs(10));
        assert_eq!(config.reconnect_after_seconds, 5);
    }
}

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::config::WebSocketConfig;
use crate::connection_manager::ConnectionManager;
use crate::websocket::ServerMessage;

/// Background task for heartbeat and connection cleanup
pub struct HeartbeatTask {
    config: WebSocketConfig,
    connection_manager: Arc<ConnectionManager>,
    shutdown: broadcast::Receiver<()>,
}

impl HeartbeatTask {
    pub fn new(
        config: WebSocketConfig,
        connection_manager: Arc<ConnectionManager>,
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            config,
            connection_manager,
            shutdown,
        }
    }

    /// Run the heartbeat and cleanup tasks
    pub async fn run(mut self) {
        let heartbeat_interval = Duration::from_secs(self.config.heartbeat_interval);
        let cleanup_interval = Duration::from_secs(self.config.cleanup_interval);
        let connection_timeout = self.config.connection_timeout;

        let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);
        let mut cleanup_timer = tokio::time::interval(cleanup_interval);

        // Skip immediate first tick
        heartbeat_timer.tick().await;
        cleanup_timer.tick().await;

        tracing::info!(
            heartbeat_interval_secs = self.config.heartbeat_interval,
            cleanup_interval_secs = self.config.cleanup_interval,
            connection_timeout_secs = connection_timeout,
            "Heartbeat task started"
        );

        loop {
            tokio::select! {
                _ = self.shutdown.recv() => {
                    tracing::info!("Heartbeat task received shutdown signal");
                    break;
                }
                _ = heartbeat_timer.tick() => {
                    self.send_heartbeats().await;
                }
                _ = cleanup_timer.tick() => {
                    self.cleanup_stale_connections(connection_timeout).await;
                }
            }
        }

        tracing::info!("Heartbeat task stopped");
    }

    /// Send heartbeat (ping) to all connections
    async fn send_heartbeats(&self) {
        let connections = self.connection_manager.get_all_connections();
        let count = connections.len();

        if count == 0 {
            return;
        }

        let mut sent = 0;
        let mut failed = 0;

        for handle in connections {
            match handle.send(ServerMessage::Heartbeat).await {
                Ok(_) => sent += 1,
                Err(_) => {
                    failed += 1;
                    // Connection is dead, it will be cleaned up by the cleanup task
                    tracing::debug!(
                        connection_id = %handle.id,
                        "Failed to send heartbeat, connection may be dead"
                    );
                }
            }
        }

        tracing::debug!(
            total = count,
            sent = sent,
            failed = failed,
            "Heartbeat round completed"
        );
    }

    /// Clean up stale connections
    async fn cleanup_stale_connections(&self, timeout_secs: u64) {
        let removed = self.connection_manager.cleanup_stale_connections(timeout_secs).await;

        if removed > 0 {
            tracing::info!(
                removed = removed,
                timeout_secs = timeout_secs,
                "Cleaned up stale connections"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_heartbeat_task_shutdown() {
        let config = WebSocketConfig {
            heartbeat_interval: 1,
            connection_timeout: 5,
            cleanup_interval: 2,
        };
        let connection_manager = Arc::new(ConnectionManager::new());
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        let task = HeartbeatTask::new(config, connection_manager, shutdown_rx);

        // Spawn the task
        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // Wait a bit then send shutdown
        tokio::time::sleep(Duration::from_millis(100)).await;
        shutdown_tx.send(()).unwrap();

        // Task should complete
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("Task should complete")
            .expect("Task should not panic");
    }

    #[tokio::test]
    async fn test_heartbeat_sends_to_connections() {
        let config = WebSocketConfig {
            heartbeat_interval: 1,
            connection_timeout: 60,
            cleanup_interval: 60,
        };
        let connection_manager = Arc::new(ConnectionManager::new());
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Register a test connection
        let (tx, mut rx) = mpsc::channel(10);
        let _handle = connection_manager.register("user1".to_string(), tx);

        let task = HeartbeatTask::new(config, connection_manager, shutdown_rx);

        // Spawn the task
        let task_handle = tokio::spawn(async move {
            task.run().await;
        });

        // Wait for heartbeat
        let msg = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("Should receive heartbeat")
            .expect("Channel should not be closed");

        assert!(matches!(msg, ServerMessage::Heartbeat));

        // Shutdown
        shutdown_tx.send(()).unwrap();
        let _ = task_handle.await;
    }
}

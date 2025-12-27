use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures::future::join_all;
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::cluster::SessionStore;
use crate::config::WebSocketConfig;
use crate::connection_manager::ConnectionManager;
use crate::metrics::{HeartbeatMetrics, MemoryMetrics};
use crate::websocket::ServerMessage;

/// Timeout for individual heartbeat send operations
const HEARTBEAT_SEND_TIMEOUT_MS: u64 = 5000;

/// Maximum concurrent heartbeat sends to avoid overwhelming the system
const MAX_CONCURRENT_HEARTBEATS: usize = 1000;

/// Background task for heartbeat and connection cleanup
pub struct HeartbeatTask {
    config: WebSocketConfig,
    connection_manager: Arc<ConnectionManager>,
    session_store: Arc<dyn SessionStore>,
    shutdown: broadcast::Receiver<()>,
}

impl HeartbeatTask {
    pub fn new(
        config: WebSocketConfig,
        connection_manager: Arc<ConnectionManager>,
        session_store: Arc<dyn SessionStore>,
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            config,
            connection_manager,
            session_store,
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
                    self.refresh_cluster_sessions().await;
                }
                _ = cleanup_timer.tick() => {
                    self.cleanup_stale_connections(connection_timeout).await;
                }
            }
        }

        tracing::info!("Heartbeat task stopped");
    }

    /// Send heartbeat (ping) to all connections in parallel with batching
    async fn send_heartbeats(&self) {
        let connections = self.connection_manager.get_all_connections();
        let total_count = connections.len();

        if total_count == 0 {
            return;
        }

        let start = Instant::now();
        let sent = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let timed_out = Arc::new(AtomicUsize::new(0));

        // Process in batches to avoid overwhelming the system
        for batch in connections.chunks(MAX_CONCURRENT_HEARTBEATS) {
            let futures: Vec<_> = batch
                .iter()
                .map(|handle| {
                    let sent = sent.clone();
                    let failed = failed.clone();
                    let timed_out = timed_out.clone();
                    let handle = handle.clone();

                    async move {
                        let send_timeout = Duration::from_millis(HEARTBEAT_SEND_TIMEOUT_MS);
                        match timeout(send_timeout, handle.send(ServerMessage::Heartbeat)).await {
                            Ok(Ok(_)) => {
                                sent.fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(Err(_)) => {
                                failed.fetch_add(1, Ordering::Relaxed);
                                tracing::debug!(
                                    connection_id = %handle.id,
                                    "Failed to send heartbeat, connection may be dead"
                                );
                            }
                            Err(_) => {
                                timed_out.fetch_add(1, Ordering::Relaxed);
                                tracing::debug!(
                                    connection_id = %handle.id,
                                    timeout_ms = HEARTBEAT_SEND_TIMEOUT_MS,
                                    "Heartbeat send timed out"
                                );
                            }
                        }
                    }
                })
                .collect();

            // Execute batch in parallel
            join_all(futures).await;
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let sent_count = sent.load(Ordering::Relaxed);
        let failed_count = failed.load(Ordering::Relaxed);
        let timed_out_count = timed_out.load(Ordering::Relaxed);

        // Record metrics
        HeartbeatMetrics::record_duration_ms(elapsed_ms);
        if timed_out_count > 0 {
            HeartbeatMetrics::record_timeouts(timed_out_count as u64);
        }

        // Update memory metrics during heartbeat
        MemoryMetrics::update_process_memory();
        MemoryMetrics::update_connection_manager_memory(
            total_count,
            self.connection_manager.total_subscriptions(),
        );

        tracing::debug!(
            total = total_count,
            sent = sent_count,
            failed = failed_count,
            timed_out = timed_out_count,
            elapsed_ms = elapsed_ms,
            "Heartbeat round completed (parallel)"
        );

        // Warn if heartbeat round is taking too long
        if elapsed_ms > (self.config.heartbeat_interval * 1000 / 2) {
            tracing::warn!(
                elapsed_ms = elapsed_ms,
                heartbeat_interval_ms = self.config.heartbeat_interval * 1000,
                connections = total_count,
                "Heartbeat round took more than 50% of interval"
            );
        }
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

    /// Refresh cluster session TTLs
    async fn refresh_cluster_sessions(&self) {
        if !self.session_store.is_enabled() {
            return;
        }

        match self.session_store.refresh_sessions().await {
            Ok(refreshed) => {
                if refreshed > 0 {
                    tracing::debug!(
                        refreshed = refreshed,
                        server_id = %self.session_store.server_id(),
                        "Refreshed cluster sessions"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to refresh cluster sessions"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::{create_session_store, ClusterConfig};
    use crate::websocket::OutboundMessage;
    use tokio::sync::mpsc;

    fn create_test_session_store() -> Arc<dyn SessionStore> {
        let config = ClusterConfig::default();
        create_session_store(&config, None)
    }

    #[tokio::test]
    async fn test_heartbeat_task_shutdown() {
        let config = WebSocketConfig::default();
        let connection_manager = Arc::new(ConnectionManager::new());
        let session_store = create_test_session_store();
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        let task = HeartbeatTask::new(config, connection_manager, session_store, shutdown_rx);

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
            ..Default::default()
        };
        let connection_manager = Arc::new(ConnectionManager::new());
        let session_store = create_test_session_store();
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Register a test connection with OutboundMessage channel
        let (tx, mut rx) = mpsc::channel::<OutboundMessage>(10);
        let _handle = connection_manager.register("user1".to_string(), "default".to_string(), vec![], tx).unwrap();

        let task = HeartbeatTask::new(config, connection_manager, session_store, shutdown_rx);

        // Spawn the task
        let task_handle = tokio::spawn(async move {
            task.run().await;
        });

        // Wait for heartbeat
        let msg = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("Should receive heartbeat")
            .expect("Channel should not be closed");

        // Check that we received a heartbeat message
        assert!(matches!(msg, OutboundMessage::Raw(ServerMessage::Heartbeat)));

        // Shutdown
        shutdown_tx.send(()).unwrap();
        let _ = task_handle.await;
    }
}

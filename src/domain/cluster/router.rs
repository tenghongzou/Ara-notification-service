//! Cluster message router for cross-server messaging
//!
//! This module handles routing notifications to users connected to other
//! server instances in a distributed deployment.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::broadcast;

use crate::cluster::{ClusterConfig, RoutedMessage, SessionStore, SessionStoreError};
use crate::connection_manager::ConnectionManager;
use crate::metrics::ClusterMetrics;
use crate::redis::pool::RedisPool;
use crate::websocket::{OutboundMessage, ServerMessage};

/// Router for handling cross-server message delivery
pub struct ClusterRouter {
    connection_manager: Arc<ConnectionManager>,
    session_store: Arc<dyn SessionStore>,
}

impl ClusterRouter {
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        session_store: Arc<dyn SessionStore>,
    ) -> Self {
        Self {
            connection_manager,
            session_store,
        }
    }

    /// Check if a user is connected locally
    pub fn is_user_local(&self, user_id: &str) -> bool {
        !self.connection_manager.get_user_connections(user_id).is_empty()
    }

    /// Route a message to a user across the cluster
    /// Returns the number of connections that received the message locally
    /// and whether the message was also routed to other servers
    pub async fn route_to_user(
        &self,
        user_id: &str,
        tenant_id: &str,
        message: ServerMessage,
    ) -> Result<RouteResult, SessionStoreError> {
        // First, try local delivery
        let local_connections = self.connection_manager.get_user_connections(user_id);

        let mut local_delivered = 0;
        if !local_connections.is_empty() {
            let outbound = OutboundMessage::Raw(message.clone());
            for conn in local_connections {
                if conn.send_preserialized(outbound.clone()).await.is_ok() {
                    local_delivered += 1;
                }
            }
        }

        // If cluster mode is enabled, check if user might be on other servers
        let routed_to_servers = if self.session_store.is_enabled() {
            match self.session_store.find_user_servers(user_id).await {
                Ok(servers) => {
                    let other_servers: Vec<_> = servers
                        .into_iter()
                        .filter(|s| s != self.session_store.server_id())
                        .collect();

                    if !other_servers.is_empty() {
                        // Route to other servers
                        let payload = serde_json::to_string(&message)
                            .map_err(|e| SessionStoreError::SerializationError(e.to_string()))?;

                        for target_server in &other_servers {
                            let routed_msg = RoutedMessage {
                                user_id: user_id.to_string(),
                                tenant_id: tenant_id.to_string(),
                                connection_id: None,
                                payload: payload.clone(),
                                from_server: self.session_store.server_id().to_string(),
                                to_server: Some(target_server.clone()),
                            };

                            if let Err(e) = self.session_store.publish_routed_message(&routed_msg).await {
                                tracing::warn!(
                                    error = %e,
                                    target_server = %target_server,
                                    user_id = %user_id,
                                    "Failed to route message to server"
                                );
                            } else {
                                ClusterMetrics::record_message_routed();
                            }
                        }
                        other_servers.len()
                    } else {
                        0
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        user_id = %user_id,
                        "Failed to find user servers for routing"
                    );
                    0
                }
            }
        } else {
            0
        };

        Ok(RouteResult {
            local_delivered,
            routed_to_servers,
        })
    }

    /// Handle a routed message received from another server
    pub async fn handle_routed_message(&self, message: RoutedMessage) -> usize {
        // Only process if targeted to this server or broadcast
        if let Some(ref target) = message.to_server {
            if target != self.session_store.server_id() {
                return 0;
            }
        }

        ClusterMetrics::record_message_received();

        // Parse the payload
        let server_message: ServerMessage = match serde_json::from_str(&message.payload) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    from_server = %message.from_server,
                    "Failed to parse routed message payload"
                );
                return 0;
            }
        };

        // Deliver locally
        let connections = self.connection_manager.get_user_connections(&message.user_id);
        let mut delivered = 0;

        let outbound = OutboundMessage::Raw(server_message);
        for conn in connections {
            if conn.send_preserialized(outbound.clone()).await.is_ok() {
                delivered += 1;
            }
        }

        tracing::debug!(
            from_server = %message.from_server,
            user_id = %message.user_id,
            delivered = delivered,
            "Handled routed message"
        );

        delivered
    }
}

/// Result of routing a message
#[derive(Debug, Clone)]
pub struct RouteResult {
    /// Number of connections delivered to locally
    pub local_delivered: usize,
    /// Number of other servers the message was routed to
    pub routed_to_servers: usize,
}

/// Background task for receiving routed messages from other servers
pub struct RoutedMessageSubscriber {
    config: ClusterConfig,
    redis_pool: Arc<RedisPool>,
    router: Arc<ClusterRouter>,
    shutdown: broadcast::Receiver<()>,
}

impl RoutedMessageSubscriber {
    pub fn new(
        config: ClusterConfig,
        redis_pool: Arc<RedisPool>,
        router: Arc<ClusterRouter>,
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            config,
            redis_pool,
            router,
            shutdown,
        }
    }

    /// Run the subscriber task with automatic reconnection
    pub async fn run(mut self) {
        if !self.config.enabled {
            tracing::info!("Cluster mode disabled, routed message subscriber not starting");
            return;
        }

        tracing::info!(
            server_id = %self.config.server_id,
            routing_channel = %self.config.routing_channel,
            "Routed message subscriber starting"
        );

        let mut retry_delay = Duration::from_millis(100);
        let max_retry_delay = Duration::from_secs(30);

        loop {
            match self.run_subscription_loop().await {
                Ok(()) => {
                    // Graceful shutdown
                    tracing::info!("Routed message subscriber stopped gracefully");
                    break;
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        retry_delay_ms = retry_delay.as_millis(),
                        "Routed message subscription error, reconnecting"
                    );

                    // Check for shutdown during retry delay
                    tokio::select! {
                        _ = self.shutdown.recv() => {
                            tracing::info!("Shutdown requested during reconnect delay");
                            break;
                        }
                        _ = tokio::time::sleep(retry_delay) => {
                            // Exponential backoff with cap
                            retry_delay = std::cmp::min(retry_delay * 2, max_retry_delay);
                        }
                    }
                }
            }
        }
    }

    /// Run the subscription loop
    async fn run_subscription_loop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create a new client for pub/sub (pub/sub requires dedicated connection)
        let url = self.redis_pool.url();
        let client = redis::Client::open(url)?;
        let mut pubsub = client.get_async_pubsub().await?;

        // Subscribe to routing channels:
        // 1. Server-specific channel: ara:cluster:route:{server_id}
        // 2. Broadcast channel: ara:cluster:route
        let server_channel = format!("{}:{}", self.config.routing_channel, self.config.server_id);
        let broadcast_channel = self.config.routing_channel.clone();

        pubsub.subscribe(&server_channel).await?;
        pubsub.subscribe(&broadcast_channel).await?;

        tracing::info!(
            server_channel = %server_channel,
            broadcast_channel = %broadcast_channel,
            "Subscribed to routing channels"
        );

        let mut message_stream = pubsub.on_message();

        loop {
            tokio::select! {
                biased;

                // Handle shutdown signal (priority)
                _ = self.shutdown.recv() => {
                    tracing::info!("Received shutdown signal");
                    return Ok(());
                }

                // Handle incoming messages
                msg = message_stream.next() => {
                    match msg {
                        Some(msg) => {
                            let channel: String = msg.get_channel_name().to_string();
                            let payload: String = match msg.get_payload() {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to get message payload");
                                    continue;
                                }
                            };

                            self.handle_routed_message(&channel, &payload).await;
                        }
                        None => {
                            tracing::warn!("Redis message stream ended unexpectedly");
                            return Err("Message stream ended".into());
                        }
                    }
                }
            }
        }
    }

    /// Handle a received routed message
    async fn handle_routed_message(&self, channel: &str, payload: &str) {
        // Parse the RoutedMessage
        let message: RoutedMessage = match serde_json::from_str(payload) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    channel = %channel,
                    "Failed to parse routed message"
                );
                return;
            }
        };

        // Skip messages from ourselves
        if message.from_server == self.config.server_id {
            return;
        }

        // Skip messages not targeted to us (if targeted)
        if let Some(ref target) = message.to_server {
            if target != &self.config.server_id {
                return;
            }
        }

        tracing::debug!(
            from_server = %message.from_server,
            user_id = %message.user_id,
            "Received routed message"
        );

        // Deliver the message locally
        let delivered = self.router.handle_routed_message(message).await;

        if delivered > 0 {
            tracing::debug!(
                delivered = delivered,
                "Delivered routed message to local connections"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::{create_session_store, ClusterConfig, RoutedMessage};

    fn create_test_components() -> (Arc<ConnectionManager>, Arc<dyn SessionStore>) {
        let connection_manager = Arc::new(ConnectionManager::new());
        let config = ClusterConfig::default();
        let session_store = create_session_store(&config, None);
        (connection_manager, session_store)
    }

    #[test]
    fn test_route_result() {
        let result = RouteResult {
            local_delivered: 3,
            routed_to_servers: 2,
        };
        assert_eq!(result.local_delivered, 3);
        assert_eq!(result.routed_to_servers, 2);
    }

    #[tokio::test]
    async fn test_is_user_local_no_connections() {
        let (connection_manager, session_store) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store);

        assert!(!router.is_user_local("nonexistent-user"));
    }

    #[tokio::test]
    async fn test_route_to_user_no_connections() {
        let (connection_manager, session_store) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store);

        let result = router.route_to_user(
            "user1",
            "tenant1",
            ServerMessage::Heartbeat,
        ).await.unwrap();

        assert_eq!(result.local_delivered, 0);
        assert_eq!(result.routed_to_servers, 0);
    }

    #[tokio::test]
    async fn test_handle_routed_message_no_local_connections() {
        let (connection_manager, session_store) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store);

        let message = RoutedMessage {
            user_id: "user1".to_string(),
            tenant_id: "tenant1".to_string(),
            connection_id: None,
            payload: r#"{"type":"Heartbeat"}"#.to_string(),
            from_server: "other-server".to_string(),
            to_server: None,
        };

        let delivered = router.handle_routed_message(message).await;
        assert_eq!(delivered, 0);
    }

    #[tokio::test]
    async fn test_handle_routed_message_wrong_server() {
        let (connection_manager, session_store) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store.clone());

        let message = RoutedMessage {
            user_id: "user1".to_string(),
            tenant_id: "tenant1".to_string(),
            connection_id: None,
            payload: r#"{"type":"Heartbeat"}"#.to_string(),
            from_server: "other-server".to_string(),
            to_server: Some("different-server".to_string()), // Not our server
        };

        // Should return 0 because message is not for this server
        let delivered = router.handle_routed_message(message).await;
        assert_eq!(delivered, 0);
    }

    #[test]
    fn test_routed_message_serialization() {
        let message = RoutedMessage {
            user_id: "user1".to_string(),
            tenant_id: "tenant1".to_string(),
            connection_id: None,
            payload: r#"{"type":"notification"}"#.to_string(),
            from_server: "server1".to_string(),
            to_server: Some("server2".to_string()),
        };

        let json = serde_json::to_string(&message).unwrap();
        let parsed: RoutedMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.user_id, message.user_id);
        assert_eq!(parsed.from_server, message.from_server);
        assert_eq!(parsed.to_server, message.to_server);
    }

    #[test]
    fn test_cluster_config_defaults() {
        let config = ClusterConfig::default();
        assert!(!config.enabled);
        assert!(config.server_id.starts_with("ara-"));
        assert_eq!(config.session_prefix, "ara:cluster:sessions");
        assert_eq!(config.session_ttl_seconds, 60);
        assert_eq!(config.routing_channel, "ara:cluster:route");
    }
}

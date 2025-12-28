//! Integration tests for cluster module
//!
//! These tests verify cross-component interactions without requiring
//! actual Redis or server startup.

use std::sync::Arc;
use uuid::Uuid;

use ara_notification_service::cluster::{
    create_session_store, ClusterConfig, ClusterRouter, RouteResult, RoutedMessage,
    SessionInfo, SessionStore, SessionStoreBackend,
};
use ara_notification_service::connection_manager::{ConnectionLimits, ConnectionManager};

/// Helper to create test components with local session store
fn create_test_components() -> (Arc<ConnectionManager>, Arc<dyn SessionStore>, ClusterConfig) {
    let limits = ConnectionLimits {
        max_connections: 1000,
        max_connections_per_user: 5,
        max_subscriptions_per_connection: 50,
    };
    let connection_manager = Arc::new(ConnectionManager::with_limits(limits));

    let config = ClusterConfig {
        enabled: false,
        server_id: "test-server-1".to_string(),
        session_prefix: "test:cluster:sessions".to_string(),
        session_ttl_seconds: 60,
        routing_channel: "test:cluster:route".to_string(),
    };

    let session_store = create_session_store(&config, None);

    (connection_manager, session_store, config)
}

/// Helper to create cluster config for specific server
fn create_cluster_config(server_id: &str, enabled: bool) -> ClusterConfig {
    ClusterConfig {
        enabled,
        server_id: server_id.to_string(),
        session_prefix: "test:cluster:sessions".to_string(),
        session_ttl_seconds: 60,
        routing_channel: "test:cluster:route".to_string(),
    }
}

// =============================================================================
// Session Store Integration Tests
// =============================================================================

mod session_store_tests {
    use super::*;

    #[test]
    fn test_create_local_session_store_when_disabled() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        assert!(!store.is_enabled());
        assert_eq!(store.backend_type(), SessionStoreBackend::Local);
        assert_eq!(store.server_id(), "server-1");
    }

    #[test]
    fn test_create_local_session_store_when_enabled_but_no_redis() {
        let config = create_cluster_config("server-1", true);
        // Without Redis pool, should fall back to local
        let store = create_session_store(&config, None);

        // Falls back to local when Redis not available
        assert!(!store.is_enabled());
        assert_eq!(store.backend_type(), SessionStoreBackend::Local);
    }

    #[tokio::test]
    async fn test_local_session_store_register_unregister() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        let session = SessionInfo {
            connection_id: Uuid::new_v4(),
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            server_id: "server-1".to_string(),
            connected_at: chrono::Utc::now().timestamp(),
            channels: vec!["orders".to_string()],
        };

        // Register should succeed (no-op)
        assert!(store.register_session(&session).await.is_ok());

        // Unregister should succeed (no-op)
        assert!(store.unregister_session(session.connection_id).await.is_ok());
    }

    #[tokio::test]
    async fn test_local_session_store_find_servers() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        // Local store always returns its own server
        let servers = store.find_user_servers("any-user").await.unwrap();
        assert_eq!(servers, vec!["server-1"]);

        let channel_servers = store.find_channel_servers("any-channel").await.unwrap();
        assert_eq!(channel_servers, vec!["server-1"]);
    }

    #[tokio::test]
    async fn test_local_session_store_routing_disabled() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        let message = RoutedMessage {
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            connection_id: None,
            payload: "{}".to_string(),
            from_server: "server-1".to_string(),
            to_server: Some("server-2".to_string()),
        };

        // Routing should fail in local mode
        let result = store.publish_routed_message(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_local_session_store_cluster_counts_disabled() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        // Cluster counts should fail in local mode
        let conn_count = store.cluster_connection_count().await;
        assert!(conn_count.is_err());

        let user_count = store.cluster_user_count().await;
        assert!(user_count.is_err());
    }

    #[tokio::test]
    async fn test_local_session_store_get_sessions() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        // Local store returns empty for distributed queries
        let all_sessions = store.get_all_sessions().await.unwrap();
        assert!(all_sessions.is_empty());

        let user_sessions = store.get_user_sessions("any-user").await.unwrap();
        assert!(user_sessions.is_empty());
    }

    #[tokio::test]
    async fn test_local_session_store_update_channels() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        let connection_id = Uuid::new_v4();
        let channels = vec!["orders".to_string(), "alerts".to_string()];

        // Update should succeed (no-op)
        let result = store.update_session_channels(connection_id, channels).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_local_session_store_refresh_sessions() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        // Refresh should succeed (no-op) and return 0
        let refreshed = store.refresh_sessions().await.unwrap();
        assert_eq!(refreshed, 0);
    }
}

// =============================================================================
// ClusterRouter Integration Tests
// =============================================================================

mod router_tests {
    use super::*;

    #[test]
    fn test_router_creation() {
        let (connection_manager, session_store, _config) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store);

        // Router should be created successfully
        assert!(!router.is_user_local("nonexistent-user"));
    }

    #[test]
    fn test_is_user_local_no_connections() {
        let (connection_manager, session_store, _config) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store);

        // Non-existent user should not be local
        assert!(!router.is_user_local("user-1"));
        assert!(!router.is_user_local(""));
    }

    #[tokio::test]
    async fn test_route_to_user_no_connections_local_mode() {
        let (connection_manager, session_store, _config) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store);

        let message = ara_notification_service::websocket::ServerMessage::Heartbeat;

        let result = router.route_to_user("user-1", "tenant-1", message).await;
        assert!(result.is_ok());

        let route_result = result.unwrap();
        assert_eq!(route_result.local_delivered, 0);
        assert_eq!(route_result.routed_to_servers, 0);
    }

    #[tokio::test]
    async fn test_handle_routed_message_no_local_connections() {
        let (connection_manager, session_store, _config) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store);

        let message = RoutedMessage {
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            connection_id: None,
            payload: r#"{"type":"Heartbeat"}"#.to_string(),
            from_server: "other-server".to_string(),
            to_server: None,
        };

        let delivered = router.handle_routed_message(message).await;
        assert_eq!(delivered, 0);
    }

    #[tokio::test]
    async fn test_handle_routed_message_wrong_target_server() {
        let config = create_cluster_config("server-1", false);
        let limits = ConnectionLimits::default();
        let connection_manager = Arc::new(ConnectionManager::with_limits(limits));
        let session_store = create_session_store(&config, None);
        let router = ClusterRouter::new(connection_manager, session_store);

        let message = RoutedMessage {
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            connection_id: None,
            payload: r#"{"type":"Heartbeat"}"#.to_string(),
            from_server: "server-2".to_string(),
            to_server: Some("server-3".to_string()), // Not our server
        };

        // Should return 0 because message is not for this server
        let delivered = router.handle_routed_message(message).await;
        assert_eq!(delivered, 0);
    }

    #[tokio::test]
    async fn test_handle_routed_message_invalid_payload() {
        let (connection_manager, session_store, _config) = create_test_components();
        let router = ClusterRouter::new(connection_manager, session_store);

        let message = RoutedMessage {
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            connection_id: None,
            payload: "invalid json".to_string(),
            from_server: "other-server".to_string(),
            to_server: None,
        };

        // Should return 0 due to parse failure
        let delivered = router.handle_routed_message(message).await;
        assert_eq!(delivered, 0);
    }
}

// =============================================================================
// Route Result Tests
// =============================================================================

mod route_result_tests {
    use super::*;

    #[test]
    fn test_route_result_creation() {
        let result = RouteResult {
            local_delivered: 5,
            routed_to_servers: 3,
        };

        assert_eq!(result.local_delivered, 5);
        assert_eq!(result.routed_to_servers, 3);
    }

    #[test]
    fn test_route_result_clone() {
        let result = RouteResult {
            local_delivered: 10,
            routed_to_servers: 2,
        };

        let cloned = result.clone();
        assert_eq!(cloned.local_delivered, result.local_delivered);
        assert_eq!(cloned.routed_to_servers, result.routed_to_servers);
    }

    #[test]
    fn test_route_result_debug() {
        let result = RouteResult {
            local_delivered: 1,
            routed_to_servers: 0,
        };

        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("local_delivered"));
        assert!(debug_str.contains("routed_to_servers"));
    }
}

// =============================================================================
// Session Info Tests
// =============================================================================

mod session_info_tests {
    use super::*;

    #[test]
    fn test_session_info_serialization_roundtrip() {
        let session = SessionInfo {
            connection_id: Uuid::new_v4(),
            user_id: "user-123".to_string(),
            tenant_id: "tenant-abc".to_string(),
            server_id: "server-1".to_string(),
            connected_at: 1234567890,
            channels: vec!["orders".to_string(), "alerts".to_string()],
        };

        let json = serde_json::to_string(&session).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.connection_id, session.connection_id);
        assert_eq!(parsed.user_id, session.user_id);
        assert_eq!(parsed.tenant_id, session.tenant_id);
        assert_eq!(parsed.server_id, session.server_id);
        assert_eq!(parsed.connected_at, session.connected_at);
        assert_eq!(parsed.channels, session.channels);
    }

    #[test]
    fn test_session_info_empty_channels() {
        let session = SessionInfo {
            connection_id: Uuid::new_v4(),
            user_id: "user-1".to_string(),
            tenant_id: "default".to_string(),
            server_id: "server-1".to_string(),
            connected_at: 0,
            channels: vec![],
        };

        let json = serde_json::to_string(&session).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
        assert!(parsed.channels.is_empty());
    }

    #[test]
    fn test_session_info_many_channels() {
        let channels: Vec<String> = (0..100).map(|i| format!("channel-{}", i)).collect();

        let session = SessionInfo {
            connection_id: Uuid::new_v4(),
            user_id: "user-1".to_string(),
            tenant_id: "default".to_string(),
            server_id: "server-1".to_string(),
            connected_at: 0,
            channels: channels.clone(),
        };

        let json = serde_json::to_string(&session).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.channels.len(), 100);
    }
}

// =============================================================================
// Routed Message Tests
// =============================================================================

mod routed_message_tests {
    use super::*;

    #[test]
    fn test_routed_message_serialization_full() {
        let message = RoutedMessage {
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            connection_id: Some(Uuid::new_v4()),
            payload: r#"{"type":"notification","data":{"title":"Hello"}}"#.to_string(),
            from_server: "server-1".to_string(),
            to_server: Some("server-2".to_string()),
        };

        let json = serde_json::to_string(&message).unwrap();
        let parsed: RoutedMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.user_id, message.user_id);
        assert_eq!(parsed.tenant_id, message.tenant_id);
        assert!(parsed.connection_id.is_some());
        assert_eq!(parsed.payload, message.payload);
        assert_eq!(parsed.from_server, message.from_server);
        assert_eq!(parsed.to_server, message.to_server);
    }

    #[test]
    fn test_routed_message_serialization_minimal() {
        let message = RoutedMessage {
            user_id: "user-1".to_string(),
            tenant_id: "default".to_string(),
            connection_id: None,
            payload: "{}".to_string(),
            from_server: "server-1".to_string(),
            to_server: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        let parsed: RoutedMessage = serde_json::from_str(&json).unwrap();

        assert!(parsed.connection_id.is_none());
        assert!(parsed.to_server.is_none());
    }

    #[test]
    fn test_routed_message_broadcast() {
        let message = RoutedMessage {
            user_id: "".to_string(), // Broadcast has no specific user
            tenant_id: "tenant-1".to_string(),
            connection_id: None,
            payload: r#"{"type":"broadcast"}"#.to_string(),
            from_server: "server-1".to_string(),
            to_server: None, // Broadcast goes to all servers
        };

        assert!(message.to_server.is_none());
        assert!(message.connection_id.is_none());
    }

    #[test]
    fn test_routed_message_clone() {
        let message = RoutedMessage {
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            connection_id: None,
            payload: "test".to_string(),
            from_server: "server-1".to_string(),
            to_server: None,
        };

        let cloned = message.clone();
        assert_eq!(cloned.user_id, message.user_id);
        assert_eq!(cloned.from_server, message.from_server);
    }
}

// =============================================================================
// Cluster Config Tests
// =============================================================================

mod config_tests {
    use super::*;

    #[test]
    fn test_cluster_config_default() {
        let config = ClusterConfig::default();

        assert!(!config.enabled);
        assert!(config.server_id.starts_with("ara-"));
        assert_eq!(config.session_prefix, "ara:cluster:sessions");
        assert_eq!(config.session_ttl_seconds, 60);
        assert_eq!(config.routing_channel, "ara:cluster:route");
    }

    #[test]
    fn test_cluster_config_custom() {
        let config = ClusterConfig {
            enabled: true,
            server_id: "custom-server".to_string(),
            session_prefix: "custom:prefix".to_string(),
            session_ttl_seconds: 120,
            routing_channel: "custom:route".to_string(),
        };

        assert!(config.enabled);
        assert_eq!(config.server_id, "custom-server");
        assert_eq!(config.session_prefix, "custom:prefix");
        assert_eq!(config.session_ttl_seconds, 120);
        assert_eq!(config.routing_channel, "custom:route");
    }

    #[test]
    fn test_cluster_config_clone() {
        let config = ClusterConfig {
            enabled: true,
            server_id: "server-1".to_string(),
            session_prefix: "prefix".to_string(),
            session_ttl_seconds: 30,
            routing_channel: "route".to_string(),
        };

        let cloned = config.clone();
        assert_eq!(cloned.enabled, config.enabled);
        assert_eq!(cloned.server_id, config.server_id);
        assert_eq!(cloned.session_ttl_seconds, config.session_ttl_seconds);
    }

    #[test]
    fn test_server_id_uniqueness() {
        let config1 = ClusterConfig::default();
        let config2 = ClusterConfig::default();

        // Each default config should have a unique server ID
        assert_ne!(config1.server_id, config2.server_id);
    }
}

// =============================================================================
// Cross-Component Integration Tests
// =============================================================================

mod cross_component_tests {
    use super::*;

    /// Test the full flow of router + session store + connection manager
    #[tokio::test]
    async fn test_router_with_local_session_store() {
        let (connection_manager, session_store, _config) = create_test_components();
        let router = Arc::new(ClusterRouter::new(
            connection_manager.clone(),
            session_store.clone(),
        ));

        // Verify initial state
        assert!(!router.is_user_local("user-1"));

        // Route to non-existent user
        let message = ara_notification_service::websocket::ServerMessage::Heartbeat;
        let result = router.route_to_user("user-1", "tenant-1", message).await;

        assert!(result.is_ok());
        let route_result = result.unwrap();
        assert_eq!(route_result.local_delivered, 0);
        assert_eq!(route_result.routed_to_servers, 0);
    }

    /// Test multiple routers with different server IDs
    #[tokio::test]
    async fn test_multiple_routers_different_servers() {
        // Create two "servers" with local session stores
        let config1 = create_cluster_config("server-1", false);
        let config2 = create_cluster_config("server-2", false);

        let limits = ConnectionLimits::default();

        let cm1 = Arc::new(ConnectionManager::with_limits(limits.clone()));
        let ss1 = create_session_store(&config1, None);
        let router1 = ClusterRouter::new(cm1.clone(), ss1.clone());

        let cm2 = Arc::new(ConnectionManager::with_limits(limits));
        let ss2 = create_session_store(&config2, None);
        let router2 = ClusterRouter::new(cm2.clone(), ss2.clone());

        // Both routers should have different server IDs
        assert_eq!(ss1.server_id(), "server-1");
        assert_eq!(ss2.server_id(), "server-2");

        // Neither should have local users
        assert!(!router1.is_user_local("user-1"));
        assert!(!router2.is_user_local("user-1"));
    }

    /// Test session store backend selection
    #[test]
    fn test_session_store_backend_selection() {
        // Local mode (disabled)
        let config_disabled = create_cluster_config("server-1", false);
        let store_disabled = create_session_store(&config_disabled, None);
        assert_eq!(store_disabled.backend_type(), SessionStoreBackend::Local);

        // Cluster mode enabled but no Redis (falls back to local)
        let config_enabled = create_cluster_config("server-2", true);
        let store_enabled = create_session_store(&config_enabled, None);
        assert_eq!(store_enabled.backend_type(), SessionStoreBackend::Local);
    }

    /// Test that connection manager and session store can be used independently
    #[tokio::test]
    async fn test_independent_component_usage() {
        let limits = ConnectionLimits::default();
        let connection_manager = Arc::new(ConnectionManager::with_limits(limits));

        let config = create_cluster_config("server-1", false);
        let session_store = create_session_store(&config, None);

        // Connection manager operations
        let user_connections = connection_manager.get_user_connections("user-1");
        assert!(user_connections.is_empty());

        let channel_connections = connection_manager.get_channel_connections("channel-1");
        assert!(channel_connections.is_empty());

        // Session store operations
        let servers = session_store.find_user_servers("user-1").await.unwrap();
        assert_eq!(servers, vec!["server-1"]);

        let refreshed = session_store.refresh_sessions().await.unwrap();
        assert_eq!(refreshed, 0);
    }
}

// =============================================================================
// Error Handling Tests
// =============================================================================

mod error_handling_tests {
    use super::*;
    use ara_notification_service::cluster::SessionStoreError;

    #[test]
    fn test_session_store_error_display() {
        let redis_error = SessionStoreError::RedisError("Connection failed".to_string());
        assert!(redis_error.to_string().contains("Redis error"));
        assert!(redis_error.to_string().contains("Connection failed"));

        let ser_error = SessionStoreError::SerializationError("Invalid JSON".to_string());
        assert!(ser_error.to_string().contains("Serialization error"));

        let disabled_error = SessionStoreError::Disabled;
        assert!(disabled_error.to_string().contains("disabled"));
    }

    #[test]
    fn test_session_store_error_clone() {
        let error = SessionStoreError::RedisError("test".to_string());
        let cloned = error.clone();

        match cloned {
            SessionStoreError::RedisError(msg) => assert_eq!(msg, "test"),
            _ => panic!("Wrong error type after clone"),
        }
    }

    #[tokio::test]
    async fn test_local_store_returns_disabled_for_routing() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        let message = RoutedMessage {
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            connection_id: None,
            payload: "{}".to_string(),
            from_server: "server-1".to_string(),
            to_server: None,
        };

        let result = store.publish_routed_message(&message).await;
        assert!(matches!(result, Err(SessionStoreError::Disabled)));
    }

    #[tokio::test]
    async fn test_local_store_returns_disabled_for_cluster_counts() {
        let config = create_cluster_config("server-1", false);
        let store = create_session_store(&config, None);

        let conn_result = store.cluster_connection_count().await;
        assert!(matches!(conn_result, Err(SessionStoreError::Disabled)));

        let user_result = store.cluster_user_count().await;
        assert!(matches!(user_result, Err(SessionStoreError::Disabled)));
    }
}

// =============================================================================
// Concurrency Tests
// =============================================================================

mod concurrency_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_concurrent_router_access() {
        let (connection_manager, session_store, _config) = create_test_components();
        let router = Arc::new(ClusterRouter::new(connection_manager, session_store));

        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn multiple concurrent tasks
        let mut handles = vec![];
        for i in 0..10 {
            let router_clone = router.clone();
            let counter_clone = counter.clone();
            let user_id = format!("user-{}", i);

            handles.push(tokio::spawn(async move {
                // Each task performs multiple operations
                for _ in 0..100 {
                    let _ = router_clone.is_user_local(&user_id);
                    let message = ara_notification_service::websocket::ServerMessage::Heartbeat;
                    let _ = router_clone
                        .route_to_user(&user_id, "tenant-1", message)
                        .await;
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        // All operations should have completed
        assert_eq!(counter.load(Ordering::SeqCst), 1000);
    }

    #[tokio::test]
    async fn test_concurrent_session_store_access() {
        let config = create_cluster_config("server-1", false);
        let store = Arc::new(create_session_store(&config, None));

        let mut handles = vec![];
        for i in 0..10 {
            let store_clone = store.clone();
            let user_id = format!("user-{}", i);

            handles.push(tokio::spawn(async move {
                for _ in 0..100 {
                    let _ = store_clone.find_user_servers(&user_id).await;
                    let _ = store_clone.refresh_sessions().await;
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }
}

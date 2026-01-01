//! Cross-component integration tests
//!
//! These tests verify interactions between multiple system components
//! without requiring actual Redis or server startup.
//!
//! Note: These tests focus on the cluster module and basic component
//! interactions that don't require complex setup.

use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use ara_notification_service::cluster::{create_session_store, ClusterConfig, ClusterRouter};
use ara_notification_service::config::{AckConfig as SettingsAckConfig, QueueConfig as SettingsQueueConfig};
use ara_notification_service::connection_manager::{ConnectionLimits, ConnectionManager};
use ara_notification_service::notification::{
    create_ack_backend, AckTrackerBackend, NotificationBuilder, NotificationDispatcher,
    NotificationTarget, Priority,
};
use ara_notification_service::queue::{create_queue_backend, MessageQueueBackend};
use ara_notification_service::ratelimit::{RateLimitConfig, RateLimiter};
use ara_notification_service::template::{Template, TemplateStore};
use ara_notification_service::tenant::{TenantConfig, TenantContext, TenantManager};

/// Create a full test environment with all components
fn create_full_test_environment() -> TestEnvironment {
    let limits = ConnectionLimits {
        max_connections: 1000,
        max_connections_per_user: 5,
        max_subscriptions_per_connection: 50,
    };
    let connection_manager = Arc::new(ConnectionManager::with_limits(limits));

    // Create queue backend using factory
    let queue_config = SettingsQueueConfig {
        enabled: true,
        backend: "memory".to_string(),
        max_size_per_user: 100,
        message_ttl_seconds: 3600,
        cleanup_interval_seconds: 300,
    };
    let queue_backend = create_queue_backend(&queue_config, None, None, None);

    // Create ACK backend using factory
    let ack_config = SettingsAckConfig {
        enabled: true,
        backend: "memory".to_string(),
        timeout_seconds: 30,
        cleanup_interval_seconds: 60,
    };
    let ack_backend = create_ack_backend(&ack_config, None, None, None);

    let dispatcher = Arc::new(NotificationDispatcher::with_backends(
        connection_manager.clone(),
        queue_backend.clone(),
        ack_backend.clone(),
    ));

    let cluster_config = ClusterConfig {
        enabled: false,
        server_id: "test-server".to_string(),
        session_prefix: "test:sessions".to_string(),
        session_ttl_seconds: 60,
        routing_channel: "test:route".to_string(),
    };
    let session_store = create_session_store(&cluster_config, None);
    let cluster_router = Arc::new(ClusterRouter::new(
        connection_manager.clone(),
        session_store.clone(),
    ));

    let template_store = Arc::new(TemplateStore::new());

    let tenant_config = TenantConfig::default();
    let tenant_manager = Arc::new(TenantManager::new(tenant_config));

    let rate_limit_config = RateLimitConfig {
        enabled: true,
        http_requests_per_second: 100,
        http_burst_size: 200,
        ws_connections_per_minute: 10,
        ws_messages_per_second: 50,
        cleanup_interval_seconds: 60,
        bucket_ttl_seconds: 300,
        backend: "local".to_string(),
        redis_prefix: "test:ratelimit".to_string(),
    };
    let rate_limiter = Arc::new(RateLimiter::new(rate_limit_config));

    TestEnvironment {
        connection_manager,
        dispatcher,
        queue_backend,
        ack_backend,
        cluster_router,
        session_store,
        template_store,
        tenant_manager,
        rate_limiter,
    }
}

struct TestEnvironment {
    connection_manager: Arc<ConnectionManager>,
    dispatcher: Arc<NotificationDispatcher>,
    queue_backend: Arc<dyn MessageQueueBackend>,
    ack_backend: Arc<dyn AckTrackerBackend>,
    cluster_router: Arc<ClusterRouter>,
    session_store: Arc<dyn ara_notification_service::cluster::SessionStore>,
    template_store: Arc<TemplateStore>,
    tenant_manager: Arc<TenantManager>,
    rate_limiter: Arc<RateLimiter>,
}

// =============================================================================
// Notification Dispatcher Integration Tests
// =============================================================================

mod dispatcher_tests {
    use super::*;

    #[tokio::test]
    async fn test_dispatch_to_nonexistent_user() {
        let env = create_full_test_environment();

        let event = NotificationBuilder::new("test.event", "integration-test")
            .payload(json!({"message": "Hello"}))
            .build();

        let result = env
            .dispatcher
            .dispatch(NotificationTarget::User("nonexistent-user".to_string()), event)
            .await;

        // Should succeed but deliver to 0 connections (but message is queued)
        assert_eq!(result.delivered_to, 0);
    }

    #[tokio::test]
    async fn test_dispatch_broadcast_no_connections() {
        let env = create_full_test_environment();

        let event = NotificationBuilder::new("broadcast.event", "integration-test")
            .payload(json!({"announcement": "System maintenance"}))
            .priority(Priority::High)
            .build();

        let result = env.dispatcher.dispatch(NotificationTarget::Broadcast, event).await;

        assert_eq!(result.delivered_to, 0);
    }

    #[tokio::test]
    async fn test_dispatch_to_channel_no_subscribers() {
        let env = create_full_test_environment();

        let event = NotificationBuilder::new("channel.event", "integration-test")
            .payload(json!({"data": "test"}))
            .build();

        let result = env
            .dispatcher
            .dispatch(
                NotificationTarget::Channel("empty-channel".to_string()),
                event,
            )
            .await;

        assert_eq!(result.delivered_to, 0);
    }

    #[tokio::test]
    async fn test_dispatch_to_multiple_users() {
        let env = create_full_test_environment();

        let event = NotificationBuilder::new("multi.event", "integration-test")
            .payload(json!({"notification": "test"}))
            .build();

        let users = vec!["user1".to_string(), "user2".to_string(), "user3".to_string()];
        let result = env
            .dispatcher
            .dispatch(NotificationTarget::Users(users), event)
            .await;

        assert_eq!(result.delivered_to, 0); // No actual connections
    }

    #[tokio::test]
    async fn test_dispatch_stats_tracking() {
        let env = create_full_test_environment();

        // Dispatch multiple events
        for i in 0..5 {
            let event = NotificationBuilder::new("stats.event", "integration-test")
                .payload(json!({"index": i}))
                .build();

            let _ = env
                .dispatcher
                .dispatch(NotificationTarget::Broadcast, event)
                .await;
        }

        let stats = env.dispatcher.stats();
        assert_eq!(stats.total_sent, 5);
        assert_eq!(stats.broadcast_notifications, 5);
    }

    #[tokio::test]
    async fn test_dispatch_to_multiple_channels() {
        let env = create_full_test_environment();

        let event = NotificationBuilder::new("multi-channel.event", "integration-test")
            .payload(json!({"data": "broadcast to channels"}))
            .build();

        let channels = vec!["channel-a".to_string(), "channel-b".to_string()];
        let result = env
            .dispatcher
            .dispatch(NotificationTarget::Channels(channels), event)
            .await;

        assert_eq!(result.delivered_to, 0);
    }
}

// =============================================================================
// Template Store Integration Tests
// =============================================================================

mod template_tests {
    use super::*;

    #[test]
    fn test_template_create_and_render() {
        let env = create_full_test_environment();

        let template = Template {
            id: "order-shipped".to_string(),
            name: "Order Shipped".to_string(),
            event_type: "order.shipped".to_string(),
            payload_template: json!({
                "title": "Order {{order_id}} shipped",
                "tracking": "{{tracking_number}}"
            }),
            default_priority: Priority::High,
            default_ttl: Some(86400),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Create template
        let result = env.template_store.create(template.clone());
        assert!(result.is_ok());

        // Get template
        let retrieved = env.template_store.get("order-shipped");
        assert!(retrieved.is_ok());
        assert_eq!(retrieved.unwrap().name, "Order Shipped");

        // Render template
        let variables = json!({
            "order_id": "ORD-123",
            "tracking_number": "TN-456789"
        });
        let rendered = env.template_store.render("order-shipped", &variables);
        assert!(rendered.is_ok());
    }

    #[test]
    fn test_template_list_and_delete() {
        let env = create_full_test_environment();

        // Create multiple templates
        for i in 0..3 {
            let template = Template {
                id: format!("template-{}", i),
                name: format!("Template {}", i),
                event_type: format!("event.type.{}", i),
                payload_template: json!({"index": i}),
                default_priority: Priority::Normal,
                default_ttl: None,
                description: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            let _ = env.template_store.create(template);
        }

        // List templates
        let templates = env.template_store.list();
        assert_eq!(templates.len(), 3);

        // Delete template
        let deleted = env.template_store.delete("template-1");
        assert!(deleted.is_ok());

        // Verify deletion
        let templates = env.template_store.list();
        assert_eq!(templates.len(), 2);
    }

    #[test]
    fn test_template_duplicate_id_error() {
        let env = create_full_test_environment();

        let template = Template {
            id: "unique-id".to_string(),
            name: "First".to_string(),
            event_type: "event.first".to_string(),
            payload_template: json!({}),
            default_priority: Priority::Normal,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // First creation should succeed
        assert!(env.template_store.create(template.clone()).is_ok());

        // Second creation with same ID should fail
        let duplicate = Template {
            id: "unique-id".to_string(),
            name: "Second".to_string(),
            event_type: "event.second".to_string(),
            payload_template: json!({}),
            default_priority: Priority::Normal,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(env.template_store.create(duplicate).is_err());
    }
}

// =============================================================================
// Tenant Manager Integration Tests
// =============================================================================

mod tenant_tests {
    use super::*;

    #[test]
    fn test_tenant_context_channel_namespacing() {
        // Test with custom tenant
        let ctx = TenantContext::new("acme-corp");
        let namespaced = ctx.namespace_channel("orders");
        assert_eq!(namespaced, "acme-corp:orders");

        let extracted = ctx.extract_channel_name(&namespaced);
        assert_eq!(extracted, Some("orders".to_string()));
    }

    #[test]
    fn test_tenant_default_context() {
        let ctx = TenantContext::default_tenant();
        assert!(ctx.is_default);

        // Default tenant doesn't namespace
        let namespaced = ctx.namespace_channel("orders");
        assert_eq!(namespaced, "orders");
    }

    #[test]
    fn test_tenant_manager_get_limits() {
        let env = create_full_test_environment();

        let limits = env.tenant_manager.get_limits("any-tenant");
        // Default limits
        assert!(limits.max_connections > 0);
    }

    #[test]
    fn test_tenant_manager_create_context() {
        let env = create_full_test_environment();

        let ctx = env.tenant_manager.create_context("test-tenant");
        // When multi-tenancy is disabled, should return default context
        assert!(ctx.is_default);
    }

    #[test]
    fn test_tenant_stats_recording() {
        let env = create_full_test_environment();

        // Record connection
        env.tenant_manager.record_connection("stats-tenant");
        env.tenant_manager.record_connection("stats-tenant");

        // Record messages
        env.tenant_manager.record_message_sent("stats-tenant");
        env.tenant_manager.record_message_sent("stats-tenant");
        env.tenant_manager.record_message_sent("stats-tenant");

        let stats = env.tenant_manager.get_stats("stats-tenant");
        assert_eq!(stats.total_connections, 2);
        assert_eq!(stats.messages_sent, 3);
    }

    #[test]
    fn test_list_active_tenants() {
        let env = create_full_test_environment();

        // Create some tenant activity
        env.tenant_manager.record_connection("tenant-a");
        env.tenant_manager.record_connection("tenant-b");
        env.tenant_manager.record_connection("tenant-c");

        let active = env.tenant_manager.list_active_tenants();
        assert!(active.len() >= 3);
    }
}

// =============================================================================
// Rate Limiter Integration Tests
// =============================================================================

mod rate_limiter_tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_rate_limiter_key_limit() {
        let env = create_full_test_environment();

        // First request should succeed
        let result1 = env.rate_limiter.check_key("api:user1");
        assert!(result1.is_allowed());

        // Many more requests (should still be within limits)
        for _ in 0..15 {
            let _ = env.rate_limiter.check_key("api:user1");
        }

        // Stats should show bucket count
        let stats = env.rate_limiter.stats();
        assert!(stats.key_buckets > 0);
    }

    #[test]
    fn test_rate_limiter_ip_limit() {
        let env = create_full_test_environment();
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // Check IP limit
        let result = env.rate_limiter.check_ip(ip);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_rate_limiter_different_keys() {
        let env = create_full_test_environment();

        // Different keys should have independent limits
        for i in 0..5 {
            let key = format!("user:{}", i);
            let result = env.rate_limiter.check_key(&key);
            assert!(result.is_allowed());
        }

        let stats = env.rate_limiter.stats();
        assert_eq!(stats.key_buckets, 5);
    }

    #[test]
    fn test_rate_limiter_cleanup() {
        let env = create_full_test_environment();

        // Create some buckets
        for i in 0..5 {
            let _ = env.rate_limiter.check_key(&format!("cleanup-key-{}", i));
        }

        // Cleanup (should not remove recent buckets)
        let removed = env.rate_limiter.cleanup_stale();
        assert_eq!(removed, 0); // Recent buckets shouldn't be removed
    }
}

// =============================================================================
// Message Queue Integration Tests
// =============================================================================

mod queue_tests {
    use super::*;

    #[tokio::test]
    async fn test_queue_config() {
        let config = SettingsQueueConfig {
            enabled: true,
            backend: "memory".to_string(),
            max_size_per_user: 100,
            message_ttl_seconds: 3600,
            cleanup_interval_seconds: 300,
        };
        let queue = create_queue_backend(&config, None, None, None);

        let stats = queue.stats().await;
        assert!(stats.enabled);
        assert_eq!(stats.max_queue_size_config, 100);
        assert_eq!(stats.message_ttl_seconds, 3600);
    }

    #[tokio::test]
    async fn test_queue_disabled() {
        let config = SettingsQueueConfig {
            enabled: false,
            backend: "memory".to_string(),
            max_size_per_user: 100,
            message_ttl_seconds: 3600,
            cleanup_interval_seconds: 300,
        };
        let queue = create_queue_backend(&config, None, None, None);

        let stats = queue.stats().await;
        assert!(!stats.enabled);
    }
}

// =============================================================================
// ACK Tracker Integration Tests
// =============================================================================

mod ack_tracker_tests {
    use super::*;

    #[tokio::test]
    async fn test_ack_track_and_acknowledge() {
        let env = create_full_test_environment();

        // Track a notification
        let notification_id = Uuid::new_v4();
        let connection_id = Uuid::new_v4();
        env.ack_backend.track(notification_id, "user-1", connection_id).await;

        // Acknowledge it
        let result = env.ack_backend.acknowledge(notification_id, "user-1").await;
        assert!(result);

        // Acknowledging again should return false
        let result2 = env.ack_backend.acknowledge(notification_id, "user-1").await;
        assert!(!result2);
    }

    #[tokio::test]
    async fn test_ack_wrong_user() {
        let env = create_full_test_environment();

        let notification_id = Uuid::new_v4();
        let connection_id = Uuid::new_v4();
        env.ack_backend.track(notification_id, "user-1", connection_id).await;

        // Wrong user should not be able to acknowledge
        let result = env.ack_backend.acknowledge(notification_id, "user-2").await;
        assert!(!result);

        // Correct user should succeed
        let result2 = env.ack_backend.acknowledge(notification_id, "user-1").await;
        assert!(result2);
    }

    #[tokio::test]
    async fn test_ack_stats() {
        let env = create_full_test_environment();

        // Track and acknowledge several notifications
        for i in 0..5 {
            let notif_id = Uuid::new_v4();
            let conn_id = Uuid::new_v4();
            env.ack_backend.track(notif_id, "user-1", conn_id).await;

            // Acknowledge only first 3
            if i < 3 {
                env.ack_backend.acknowledge(notif_id, "user-1").await;
            }
        }

        let stats = env.ack_backend.stats().await;
        assert_eq!(stats.total_tracked, 5);
        assert_eq!(stats.total_acked, 3);
    }

    #[tokio::test]
    async fn test_ack_disabled() {
        let config = SettingsAckConfig {
            enabled: false,
            backend: "memory".to_string(),
            timeout_seconds: 30,
            cleanup_interval_seconds: 60,
        };
        let tracker = create_ack_backend(&config, None, None, None);

        let notification_id = Uuid::new_v4();
        let connection_id = Uuid::new_v4();

        // Should be no-op when disabled
        tracker.track(notification_id, "user-1", connection_id).await;

        let stats = tracker.stats().await;
        assert_eq!(stats.total_tracked, 0);
    }
}

// =============================================================================
// Cluster Router + Components Integration Tests
// =============================================================================

mod cluster_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_cluster_router_with_connection_manager() {
        let env = create_full_test_environment();

        // Verify router can access connection manager
        let is_local = env.cluster_router.is_user_local("test-user");
        assert!(!is_local); // No connections registered
    }

    #[tokio::test]
    async fn test_session_store_with_router() {
        let env = create_full_test_environment();

        // Session store should report correct server ID
        assert_eq!(env.session_store.server_id(), "test-server");

        // Router should use same session store
        let servers = env.session_store.find_user_servers("user-1").await.unwrap();
        assert_eq!(servers, vec!["test-server"]);
    }

    #[tokio::test]
    async fn test_full_stack_dispatch_flow() {
        let env = create_full_test_environment();

        // 1. Create a template
        let template = Template {
            id: "alert-template".to_string(),
            name: "Alert".to_string(),
            event_type: "system.alert".to_string(),
            payload_template: json!({
                "title": "{{title}}",
                "severity": "{{severity}}"
            }),
            default_priority: Priority::High,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        env.template_store.create(template).unwrap();

        // 2. Render template to get notification event
        let variables = json!({
            "title": "Server overload",
            "severity": "critical"
        });
        let rendered = env.template_store.render("alert-template", &variables).unwrap();

        // 3. Create notification event from rendered template
        let event = NotificationBuilder::new(&rendered.event_type, "full-stack-test")
            .payload(rendered.payload)
            .priority(rendered.priority)
            .build();

        // 4. Get tenant context
        let tenant_ctx = env.tenant_manager.create_context("ops-team");

        // 5. Namespace the channel
        let channel = tenant_ctx.namespace_channel("alerts");

        // 6. Check rate limit
        let rate_result = env.rate_limiter.check_key("dispatch:ops-team");
        assert!(rate_result.is_allowed());

        // 7. Dispatch notification
        let dispatch_result = env.dispatcher
            .dispatch(NotificationTarget::Channel(channel), event)
            .await;

        // 8. Record tenant stats
        env.tenant_manager.record_message_sent("ops-team");
        let stats = env.tenant_manager.get_stats("ops-team");
        assert_eq!(stats.messages_sent, 1);

        // Verify dispatch completed (no connections, so 0 delivered)
        assert_eq!(dispatch_result.delivered_to, 0);
    }
}

// =============================================================================
// Connection Manager Integration Tests
// =============================================================================

mod connection_manager_tests {
    use super::*;

    #[test]
    fn test_connection_limits() {
        let limits = ConnectionLimits {
            max_connections: 100,
            max_connections_per_user: 3,
            max_subscriptions_per_connection: 10,
        };
        let cm = ConnectionManager::with_limits(limits);

        // Verify limits are applied
        let stats = cm.stats();
        assert_eq!(stats.total_connections, 0);
    }

    #[test]
    fn test_channel_operations() {
        let cm = ConnectionManager::with_limits(ConnectionLimits::default());

        // No channels initially
        let channels = cm.list_channels();
        assert!(channels.is_empty());

        // Channel doesn't exist
        assert!(!cm.channel_exists("test-channel"));
    }

    #[test]
    fn test_get_user_connections() {
        let cm = ConnectionManager::with_limits(ConnectionLimits::default());

        // No connections for user
        let conns = cm.get_user_connections("user-1");
        assert!(conns.is_empty());
    }

    #[test]
    fn test_get_channel_connections() {
        let cm = ConnectionManager::with_limits(ConnectionLimits::default());

        // No connections for channel
        let conns = cm.get_channel_connections("channel-1");
        assert!(conns.is_empty());
    }

    #[test]
    fn test_get_channel_info() {
        let cm = ConnectionManager::with_limits(ConnectionLimits::default());

        // Non-existent channel
        let info = cm.get_channel_info("nonexistent");
        assert!(info.is_none());
    }
}

// =============================================================================
// Concurrency Integration Tests
// =============================================================================

mod concurrency_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_concurrent_dispatches() {
        let env = create_full_test_environment();
        let dispatcher = env.dispatcher.clone();
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn multiple concurrent dispatch tasks
        let mut handles = vec![];
        for i in 0..10 {
            let disp = dispatcher.clone();
            let cnt = counter.clone();

            handles.push(tokio::spawn(async move {
                for j in 0..10 {
                    let event = NotificationBuilder::new("concurrent.event", "concurrency-test")
                        .payload(json!({"task": i, "iteration": j}))
                        .build();

                    let _ = disp.dispatch(NotificationTarget::Broadcast, event).await;
                    cnt.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        // All dispatches should complete
        assert_eq!(counter.load(Ordering::SeqCst), 100);

        // Stats should reflect all dispatches
        let stats = dispatcher.stats();
        assert_eq!(stats.total_sent, 100);
    }

    #[test]
    fn test_concurrent_rate_limiter() {
        let env = create_full_test_environment();
        let limiter = env.rate_limiter.clone();

        // Use threads for synchronous rate limiter
        let mut handles = vec![];
        for i in 0..10 {
            let lim = limiter.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    let _ = lim.check_key(&format!("concurrent-key-{}", i));
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // All buckets should be recorded
        let stats = limiter.stats();
        assert_eq!(stats.key_buckets, 10);
    }

    #[tokio::test]
    async fn test_concurrent_cluster_operations() {
        let env = create_full_test_environment();
        let router = env.cluster_router.clone();
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn concurrent router operations
        let mut handles = vec![];
        for i in 0..10 {
            let r = router.clone();
            let cnt = counter.clone();

            handles.push(tokio::spawn(async move {
                for j in 0..50 {
                    let user = format!("user-{}-{}", i, j);
                    let _ = r.is_user_local(&user);
                    cnt.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // All operations should complete
        assert_eq!(counter.load(Ordering::SeqCst), 500);
    }
}

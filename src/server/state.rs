use std::sync::Arc;

use crate::auth::JwtValidator;
use crate::config::Settings;
use crate::connection_manager::{ConnectionLimits, ConnectionManager};
use crate::notification::{AckConfig, AckTracker, NotificationDispatcher};
use crate::queue::{QueueConfig, UserMessageQueue};
use crate::ratelimit::RateLimiter;
use crate::redis::{CircuitBreaker, CircuitBreakerConfig, RedisHealth};
use crate::template::TemplateStore;
use crate::tenant::TenantManager;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub jwt_validator: Arc<JwtValidator>,
    pub connection_manager: Arc<ConnectionManager>,
    pub dispatcher: Arc<NotificationDispatcher>,
    pub message_queue: Arc<UserMessageQueue>,
    pub rate_limiter: Arc<RateLimiter>,
    pub redis_circuit_breaker: Arc<CircuitBreaker>,
    pub redis_health: Arc<RedisHealth>,
    pub ack_tracker: Arc<AckTracker>,
    pub template_store: Arc<TemplateStore>,
    pub tenant_manager: Arc<TenantManager>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        let jwt_validator = Arc::new(JwtValidator::new(&settings.jwt));

        // Create connection manager with limits from config
        let limits = ConnectionLimits {
            max_connections: settings.websocket.max_connections,
            max_connections_per_user: settings.websocket.max_connections_per_user,
            max_subscriptions_per_connection: settings.websocket.max_subscriptions_per_connection,
        };
        let connection_manager = Arc::new(ConnectionManager::with_limits(limits));

        // Create message queue from config
        let queue_config = QueueConfig {
            enabled: settings.queue.enabled,
            max_queue_size_per_user: settings.queue.max_size_per_user,
            message_ttl_seconds: settings.queue.message_ttl_seconds,
            cleanup_interval_seconds: settings.queue.cleanup_interval_seconds,
        };
        let message_queue = Arc::new(UserMessageQueue::new(queue_config));

        // Create ACK tracker
        let ack_config = AckConfig {
            enabled: settings.ack.enabled,
            timeout_seconds: settings.ack.timeout_seconds,
            cleanup_interval_seconds: settings.ack.cleanup_interval_seconds,
        };
        let ack_tracker = Arc::new(AckTracker::with_config(ack_config));

        // Create dispatcher with message queue and ACK tracker
        let dispatcher = Arc::new(NotificationDispatcher::with_config(
            connection_manager.clone(),
            message_queue.clone(),
            ack_tracker.clone(),
        ));

        // Create rate limiter from config
        let rate_limiter = Arc::new(RateLimiter::new(crate::ratelimit::RateLimitConfig {
            enabled: settings.ratelimit.enabled,
            http_requests_per_second: settings.ratelimit.http_requests_per_second,
            http_burst_size: settings.ratelimit.http_burst_size,
            ws_connections_per_minute: settings.ratelimit.ws_connections_per_minute,
            ws_messages_per_second: settings.ratelimit.ws_messages_per_second,
            cleanup_interval_seconds: settings.ratelimit.cleanup_interval_seconds,
            bucket_ttl_seconds: 300, // 5 minutes default
        }));

        // Create Redis circuit breaker
        let cb_config = CircuitBreakerConfig {
            failure_threshold: settings.redis.circuit_breaker_failure_threshold,
            success_threshold: settings.redis.circuit_breaker_success_threshold,
            reset_timeout_ms: settings.redis.circuit_breaker_reset_timeout_seconds * 1000,
        };
        let redis_circuit_breaker = Arc::new(CircuitBreaker::with_config(cb_config));
        let redis_health = Arc::new(RedisHealth::new());

        // Create template store
        let template_store = Arc::new(TemplateStore::new());

        // Create tenant manager
        let tenant_manager = Arc::new(TenantManager::new(settings.tenant.clone()));

        Self {
            settings: Arc::new(settings),
            jwt_validator,
            connection_manager,
            dispatcher,
            message_queue,
            rate_limiter,
            redis_circuit_breaker,
            redis_health,
            ack_tracker,
            template_store,
            tenant_manager,
        }
    }
}

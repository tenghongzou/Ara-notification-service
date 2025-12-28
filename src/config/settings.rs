use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Deserializer};
use std::env;

use crate::cluster::ClusterConfig;
use crate::tenant::TenantConfig;

/// Deserialize a comma-separated string into a Vec<String>
fn deserialize_comma_separated<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    Ok(s.map(|s| {
        s.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
    .unwrap_or_default())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server: ServerConfig,
    pub jwt: JwtConfig,
    pub redis: RedisConfig,
    pub api: ApiConfig,
    #[serde(default)]
    pub websocket: WebSocketConfig,
    #[serde(default)]
    pub queue: QueueConfig,
    #[serde(default)]
    pub ratelimit: RateLimitConfig,
    #[serde(default)]
    pub ack: AckSettingsConfig,
    #[serde(default)]
    pub otel: OtelConfig,
    #[serde(default)]
    pub tenant: TenantConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub cluster: ClusterConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    /// Whether rate limiting is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Maximum HTTP requests per second (per API key or IP)
    #[serde(default = "default_http_requests_per_second")]
    pub http_requests_per_second: u32,
    /// Burst capacity for HTTP requests
    #[serde(default = "default_http_burst_size")]
    pub http_burst_size: u32,
    /// Maximum WebSocket connections per minute (per IP)
    #[serde(default = "default_ws_connections_per_minute")]
    pub ws_connections_per_minute: u32,
    /// Maximum WebSocket messages per second (per connection)
    #[serde(default = "default_ws_messages_per_second")]
    pub ws_messages_per_second: u32,
    /// Cleanup interval for stale buckets (seconds)
    #[serde(default = "default_ratelimit_cleanup_interval")]
    pub cleanup_interval_seconds: u64,
    /// Backend type: "local" or "redis" (default: "local")
    #[serde(default = "default_ratelimit_backend")]
    pub backend: String,
    /// Redis key prefix for rate limit data
    #[serde(default = "default_ratelimit_redis_prefix")]
    pub redis_prefix: String,
}

fn default_ratelimit_backend() -> String {
    "local".to_string()
}

fn default_ratelimit_redis_prefix() -> String {
    "ara:ratelimit".to_string()
}

fn default_http_requests_per_second() -> u32 {
    100
}

fn default_http_burst_size() -> u32 {
    200
}

fn default_ws_connections_per_minute() -> u32 {
    10
}

fn default_ws_messages_per_second() -> u32 {
    50
}

fn default_ratelimit_cleanup_interval() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize)]
pub struct AckSettingsConfig {
    /// Whether ACK tracking is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Timeout in seconds for pending ACKs (default: 30)
    #[serde(default = "default_ack_timeout")]
    pub timeout_seconds: u64,
    /// Cleanup interval for expired ACKs in seconds (default: 60)
    #[serde(default = "default_ack_cleanup_interval")]
    pub cleanup_interval_seconds: u64,
    /// Backend type: "memory" or "redis" (default: "memory")
    #[serde(default = "default_ack_backend")]
    pub backend: String,
    /// Redis key prefix for ACK data (default: "ara:ack")
    #[serde(default = "default_ack_redis_prefix")]
    pub redis_prefix: String,
}

fn default_ack_timeout() -> u64 {
    30
}

fn default_ack_cleanup_interval() -> u64 {
    60
}

fn default_ack_backend() -> String {
    "memory".to_string()
}

fn default_ack_redis_prefix() -> String {
    "ara:ack".to_string()
}

impl Default for AckSettingsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_seconds: default_ack_timeout(),
            cleanup_interval_seconds: default_ack_cleanup_interval(),
            backend: default_ack_backend(),
            redis_prefix: default_ack_redis_prefix(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OtelConfig {
    /// Whether OpenTelemetry tracing is enabled
    #[serde(default)]
    pub enabled: bool,
    /// OTLP exporter endpoint (gRPC)
    #[serde(default = "default_otel_endpoint")]
    pub endpoint: String,
    /// Service name for tracing
    #[serde(default = "default_otel_service_name")]
    pub service_name: String,
    /// Sampling ratio (0.0 to 1.0)
    #[serde(default = "default_otel_sampling_ratio")]
    pub sampling_ratio: f64,
}

fn default_otel_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_otel_service_name() -> String {
    "ara-notification-service".to_string()
}

fn default_otel_sampling_ratio() -> f64 {
    1.0 // Sample all traces by default
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_otel_endpoint(),
            service_name: default_otel_service_name(),
            sampling_ratio: default_otel_sampling_ratio(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebSocketConfig {
    /// Heartbeat interval in seconds (server sends ping)
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
    /// Connection timeout in seconds (disconnect if no activity)
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
    /// Cleanup task interval in seconds
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval: u64,
    /// Maximum total connections allowed (0 = unlimited)
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// Maximum connections per user (0 = unlimited)
    #[serde(default = "default_max_connections_per_user")]
    pub max_connections_per_user: usize,
    /// Maximum channel subscriptions per connection (0 = unlimited)
    #[serde(default = "default_max_subscriptions")]
    pub max_subscriptions_per_connection: usize,
}

fn default_heartbeat_interval() -> u64 {
    30 // 30 seconds
}

fn default_connection_timeout() -> u64 {
    120 // 2 minutes
}

fn default_cleanup_interval() -> u64 {
    60 // 1 minute
}

fn default_max_connections() -> usize {
    10000 // 10K connections max
}

fn default_max_connections_per_user() -> usize {
    5 // 5 connections per user
}

fn default_max_subscriptions() -> usize {
    50 // 50 channels per connection
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    /// Whether offline message queue is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Maximum number of messages to queue per user
    #[serde(default = "default_queue_max_size")]
    pub max_size_per_user: usize,
    /// Time-to-live for queued messages in seconds
    #[serde(default = "default_queue_ttl")]
    pub message_ttl_seconds: u64,
    /// Interval for cleanup task in seconds
    #[serde(default = "default_queue_cleanup_interval")]
    pub cleanup_interval_seconds: u64,
    /// Backend type: "memory" or "redis" (default: "memory")
    #[serde(default = "default_queue_backend")]
    pub backend: String,
    /// Redis key prefix for queue data (default: "ara:queue")
    #[serde(default = "default_queue_redis_prefix")]
    pub redis_prefix: String,
}

fn default_queue_max_size() -> usize {
    100 // 100 messages per user
}

fn default_queue_ttl() -> u64 {
    3600 // 1 hour
}

fn default_queue_cleanup_interval() -> u64 {
    300 // 5 minutes
}

fn default_queue_backend() -> String {
    "memory".to_string()
}

fn default_queue_redis_prefix() -> String {
    "ara:queue".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default, deserialize_with = "deserialize_comma_separated")]
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub issuer: Option<String>,
    pub audience: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
    #[serde(default, deserialize_with = "deserialize_comma_separated")]
    pub channels: Vec<String>,
    /// Circuit breaker failure threshold (consecutive failures before opening)
    #[serde(default = "default_circuit_breaker_failure_threshold")]
    pub circuit_breaker_failure_threshold: u32,
    /// Circuit breaker success threshold (successes in half-open before closing)
    #[serde(default = "default_circuit_breaker_success_threshold")]
    pub circuit_breaker_success_threshold: u32,
    /// Circuit breaker reset timeout in seconds
    #[serde(default = "default_circuit_breaker_reset_timeout")]
    pub circuit_breaker_reset_timeout_seconds: u64,
    /// Initial backoff delay in milliseconds
    #[serde(default = "default_backoff_initial_delay")]
    pub backoff_initial_delay_ms: u64,
    /// Maximum backoff delay in milliseconds
    #[serde(default = "default_backoff_max_delay")]
    pub backoff_max_delay_ms: u64,
}

fn default_circuit_breaker_failure_threshold() -> u32 {
    5
}

fn default_circuit_breaker_success_threshold() -> u32 {
    2
}

fn default_circuit_breaker_reset_timeout() -> u64 {
    30 // 30 seconds
}

fn default_backoff_initial_delay() -> u64 {
    100 // 100ms
}

fn default_backoff_max_delay() -> u64 {
    30_000 // 30 seconds
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URL
    #[serde(default = "default_database_url")]
    pub url: String,
    /// Maximum number of connections in the pool
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    /// Connection timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_seconds: u32,
    /// Idle connection timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_seconds: u32,
}

fn default_database_url() -> String {
    "postgres://localhost:5432/ara_notification".to_string()
}

fn default_pool_size() -> u32 {
    10
}

fn default_connect_timeout() -> u32 {
    30
}

fn default_idle_timeout() -> u32 {
    600 // 10 minutes
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_database_url(),
            pool_size: default_pool_size(),
            connect_timeout_seconds: default_connect_timeout(),
            idle_timeout_seconds: default_idle_timeout(),
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8081
}

fn default_redis_url() -> String {
    "redis://localhost:6379".to_string()
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        // Load .env file if exists
        let _ = dotenvy::dotenv();

        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let builder = Config::builder()
            // Start with default values
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8081)?
            .set_default("redis.url", "redis://localhost:6379")?
            .set_default("websocket.heartbeat_interval", 30)?
            .set_default("websocket.connection_timeout", 120)?
            .set_default("websocket.cleanup_interval", 60)?
            .set_default("websocket.max_connections", 10000)?
            .set_default("websocket.max_connections_per_user", 5)?
            .set_default("websocket.max_subscriptions_per_connection", 50)?
            .set_default("queue.enabled", false)?
            .set_default("queue.max_size_per_user", 100)?
            .set_default("queue.message_ttl_seconds", 3600)?
            .set_default("queue.cleanup_interval_seconds", 300)?
            .set_default("queue.backend", "memory")?
            .set_default("queue.redis_prefix", "ara:queue")?
            .set_default("ratelimit.enabled", false)?
            .set_default("ratelimit.http_requests_per_second", 100)?
            .set_default("ratelimit.http_burst_size", 200)?
            .set_default("ratelimit.ws_connections_per_minute", 10)?
            .set_default("ratelimit.ws_messages_per_second", 50)?
            .set_default("ratelimit.cleanup_interval_seconds", 60)?
            .set_default("redis.circuit_breaker_failure_threshold", 5)?
            .set_default("redis.circuit_breaker_success_threshold", 2)?
            .set_default("redis.circuit_breaker_reset_timeout_seconds", 30)?
            .set_default("redis.backoff_initial_delay_ms", 100)?
            .set_default("redis.backoff_max_delay_ms", 30000)?
            .set_default("ack.enabled", false)?
            .set_default("ack.timeout_seconds", 30)?
            .set_default("ack.cleanup_interval_seconds", 60)?
            .set_default("ack.backend", "memory")?
            .set_default("ack.redis_prefix", "ara:ack")?
            .set_default("otel.enabled", false)?
            .set_default("otel.endpoint", "http://localhost:4317")?
            .set_default("otel.service_name", "ara-notification-service")?
            .set_default("otel.sampling_ratio", 1.0)?
            .set_default("tenant.enabled", false)?
            .set_default("tenant.default_limits.max_connections", 1000)?
            .set_default("tenant.default_limits.max_connections_per_user", 5)?
            .set_default("tenant.default_limits.max_subscriptions_per_connection", 50)?
            .set_default("database.url", "postgres://localhost:5432/ara_notification")?
            .set_default("database.pool_size", 10)?
            .set_default("database.connect_timeout_seconds", 30)?
            .set_default("database.idle_timeout_seconds", 600)?
            // Cluster mode defaults
            .set_default("cluster.enabled", false)?
            .set_default("cluster.session_prefix", "ara:cluster:sessions")?
            .set_default("cluster.session_ttl_seconds", 60)?
            .set_default("cluster.routing_channel", "ara:cluster:route")?
            // Load config file if exists
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{}", run_mode)).required(false))
            // Load from environment variables
            // SERVER_HOST, SERVER_PORT, JWT_SECRET, REDIS_URL, etc.
            .add_source(
                Environment::default()
                    .separator("_")
                    .try_parsing(true),
            );

        builder.build()?.try_deserialize()
    }

    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            cors_origins: vec![],
        }
    }
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
            channels: vec![],
            circuit_breaker_failure_threshold: default_circuit_breaker_failure_threshold(),
            circuit_breaker_success_threshold: default_circuit_breaker_success_threshold(),
            circuit_breaker_reset_timeout_seconds: default_circuit_breaker_reset_timeout(),
            backoff_initial_delay_ms: default_backoff_initial_delay(),
            backoff_max_delay_ms: default_backoff_max_delay(),
        }
    }
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: default_heartbeat_interval(),
            connection_timeout: default_connection_timeout(),
            cleanup_interval: default_cleanup_interval(),
            max_connections: default_max_connections(),
            max_connections_per_user: default_max_connections_per_user(),
            max_subscriptions_per_connection: default_max_subscriptions(),
        }
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_size_per_user: default_queue_max_size(),
            message_ttl_seconds: default_queue_ttl(),
            cleanup_interval_seconds: default_queue_cleanup_interval(),
            backend: default_queue_backend(),
            redis_prefix: default_queue_redis_prefix(),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            http_requests_per_second: default_http_requests_per_second(),
            http_burst_size: default_http_burst_size(),
            ws_connections_per_minute: default_ws_connections_per_minute(),
            ws_messages_per_second: default_ws_messages_per_second(),
            cleanup_interval_seconds: default_ratelimit_cleanup_interval(),
            backend: default_ratelimit_backend(),
            redis_prefix: default_ratelimit_redis_prefix(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let server = ServerConfig::default();
        assert_eq!(server.host, "0.0.0.0");
        assert_eq!(server.port, 8081);
    }
}

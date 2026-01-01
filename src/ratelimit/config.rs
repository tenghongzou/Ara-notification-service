//! Rate limiting configuration

use serde::Deserialize;

/// Configuration for rate limiting
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    /// Whether rate limiting is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Maximum requests per second for HTTP API (per API key or IP)
    #[serde(default = "default_http_requests_per_second")]
    pub http_requests_per_second: u32,
    /// Burst capacity for HTTP requests (allows short bursts above the rate)
    #[serde(default = "default_http_burst_size")]
    pub http_burst_size: u32,
    /// Maximum WebSocket connections per minute (per IP)
    #[serde(default = "default_ws_connections_per_minute")]
    pub ws_connections_per_minute: u32,
    /// Maximum WebSocket messages per second (per connection)
    #[serde(default = "default_ws_messages_per_second")]
    pub ws_messages_per_second: u32,
    /// Cleanup interval for stale buckets in seconds
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_seconds: u64,
    /// Time after which unused buckets are removed (seconds)
    #[serde(default = "default_bucket_ttl")]
    pub bucket_ttl_seconds: u64,
    /// Backend type: "local" or "redis" (default: "local")
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Redis key prefix for rate limit data
    #[serde(default = "default_redis_prefix")]
    pub redis_prefix: String,
}

fn default_backend() -> String {
    "local".to_string()
}

fn default_redis_prefix() -> String {
    "ara:ratelimit".to_string()
}

fn default_http_requests_per_second() -> u32 {
    100 // 100 requests per second per API key
}

fn default_http_burst_size() -> u32 {
    200 // Allow burst of 200 requests
}

fn default_ws_connections_per_minute() -> u32 {
    10 // 10 connections per minute per IP
}

fn default_ws_messages_per_second() -> u32 {
    50 // 50 messages per second per connection
}

fn default_cleanup_interval() -> u64 {
    60 // Clean up every minute
}

fn default_bucket_ttl() -> u64 {
    300 // Remove buckets unused for 5 minutes
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            http_requests_per_second: default_http_requests_per_second(),
            http_burst_size: default_http_burst_size(),
            ws_connections_per_minute: default_ws_connections_per_minute(),
            ws_messages_per_second: default_ws_messages_per_second(),
            cleanup_interval_seconds: default_cleanup_interval(),
            bucket_ttl_seconds: default_bucket_ttl(),
            backend: default_backend(),
            redis_prefix: default_redis_prefix(),
        }
    }
}

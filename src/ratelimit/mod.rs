//! Rate limiting module using Token Bucket algorithm.
//!
//! This module provides rate limiting for both HTTP API requests and WebSocket connections
//! to protect against resource exhaustion attacks.
//!
//! Supports both local (in-memory) and distributed (Redis) backends.

use std::net::IpAddr;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use dashmap::DashMap;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::redis::pool::RedisPool;

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

/// Token Bucket for rate limiting.
///
/// Uses atomic operations for lock-free concurrent access.
/// Tokens are refilled at a constant rate up to the bucket capacity.
#[derive(Debug)]
pub struct TokenBucket {
    /// Current number of tokens (scaled by 1000 for precision)
    tokens: AtomicU32,
    /// Last refill timestamp (Unix milliseconds)
    last_refill: AtomicI64,
    /// Maximum bucket capacity
    capacity: u32,
    /// Tokens added per second
    refill_rate: u32,
}

impl TokenBucket {
    /// Create a new token bucket
    pub fn new(capacity: u32, refill_rate: u32) -> Self {
        Self {
            tokens: AtomicU32::new(capacity),
            last_refill: AtomicI64::new(Self::now_millis()),
            capacity,
            refill_rate,
        }
    }

    /// Get current time in milliseconds
    fn now_millis() -> i64 {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    /// Try to consume one token from the bucket.
    /// Returns true if a token was available, false otherwise.
    pub fn try_consume(&self) -> bool {
        self.try_consume_n(1)
    }

    /// Try to consume n tokens from the bucket.
    /// Returns true if tokens were available, false otherwise.
    pub fn try_consume_n(&self, n: u32) -> bool {
        let now = Self::now_millis();
        let last = self.last_refill.load(Ordering::Relaxed);
        let elapsed_ms = (now - last).max(0) as u64;

        // Calculate tokens to add based on elapsed time
        let tokens_to_add = (elapsed_ms * self.refill_rate as u64 / 1000) as u32;

        // Try to refill and consume atomically
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            let refilled = (current + tokens_to_add).min(self.capacity);

            if refilled < n {
                return false;
            }

            let new_value = refilled - n;

            // Try to update tokens
            if self
                .tokens
                .compare_exchange_weak(current, new_value, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                // Update last refill time
                self.last_refill.store(now, Ordering::Relaxed);
                return true;
            }
            // CAS failed, retry
        }
    }

    /// Get the current number of available tokens
    pub fn available(&self) -> u32 {
        let now = Self::now_millis();
        let last = self.last_refill.load(Ordering::Relaxed);
        let elapsed_ms = (now - last).max(0) as u64;
        let tokens_to_add = (elapsed_ms * self.refill_rate as u64 / 1000) as u32;
        let current = self.tokens.load(Ordering::Relaxed);
        (current + tokens_to_add).min(self.capacity)
    }

    /// Get seconds until the bucket has at least one token
    pub fn retry_after(&self) -> u64 {
        if self.available() > 0 {
            return 0;
        }
        // Time to get 1 token
        let ms_per_token = 1000 / self.refill_rate.max(1);
        (ms_per_token / 1000).max(1) as u64
    }

    /// Get the last activity time
    pub fn last_activity(&self) -> i64 {
        self.last_refill.load(Ordering::Relaxed)
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed {
        remaining: u32,
        limit: u32,
        reset_at: i64,
    },
    /// Request is denied due to rate limiting
    Denied {
        retry_after: u64,
        limit: u32,
        reset_at: i64,
    },
}

impl RateLimitResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitResult::Allowed { .. })
    }
}

/// Rate limiter entry with metadata
struct BucketEntry {
    bucket: TokenBucket,
    #[allow(dead_code)]
    created_at: Instant, // Useful for debugging and future features
}

impl BucketEntry {
    fn new(capacity: u32, refill_rate: u32) -> Self {
        Self {
            bucket: TokenBucket::new(capacity, refill_rate),
            created_at: Instant::now(),
        }
    }
}

/// Main rate limiter that manages multiple token buckets.
///
/// Supports:
/// - IP-based rate limiting for WebSocket connections
/// - API key based rate limiting for HTTP requests
/// - Per-connection rate limiting for WebSocket messages
pub struct RateLimiter {
    /// IP-based buckets for connection limiting
    ip_buckets: DashMap<IpAddr, BucketEntry>,
    /// API key / user based buckets for HTTP requests
    key_buckets: DashMap<String, BucketEntry>,
    /// Configuration
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            ip_buckets: DashMap::new(),
            key_buckets: DashMap::new(),
            config,
        }
    }

    /// Check if rate limiting is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the configuration
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Check rate limit for an IP address (WebSocket connections).
    /// Returns the result of the rate limit check.
    pub fn check_ip(&self, ip: IpAddr) -> RateLimitResult {
        if !self.config.enabled {
            return RateLimitResult::Allowed {
                remaining: u32::MAX,
                limit: 0,
                reset_at: 0,
            };
        }

        let limit = self.config.ws_connections_per_minute;
        // Refill rate: connections per minute -> tokens per second
        let refill_rate = (limit as f64 / 60.0).ceil() as u32;

        let entry = self
            .ip_buckets
            .entry(ip)
            .or_insert_with(|| BucketEntry::new(limit, refill_rate.max(1)));

        let bucket = &entry.bucket;
        let reset_at = bucket.last_activity() + 60_000; // Reset after 1 minute

        if bucket.try_consume() {
            RateLimitResult::Allowed {
                remaining: bucket.available(),
                limit,
                reset_at,
            }
        } else {
            RateLimitResult::Denied {
                retry_after: 60, // Wait up to 1 minute for connection limit
                limit,
                reset_at,
            }
        }
    }

    /// Check rate limit for an API key or identifier (HTTP requests).
    /// Returns the result of the rate limit check.
    pub fn check_key(&self, key: &str) -> RateLimitResult {
        if !self.config.enabled {
            return RateLimitResult::Allowed {
                remaining: u32::MAX,
                limit: 0,
                reset_at: 0,
            };
        }

        let limit = self.config.http_burst_size;
        let refill_rate = self.config.http_requests_per_second;

        let entry = self
            .key_buckets
            .entry(key.to_string())
            .or_insert_with(|| BucketEntry::new(limit, refill_rate));

        let bucket = &entry.bucket;
        let reset_at = bucket.last_activity() + 1_000; // Reset after 1 second

        if bucket.try_consume() {
            RateLimitResult::Allowed {
                remaining: bucket.available(),
                limit: self.config.http_requests_per_second,
                reset_at,
            }
        } else {
            let retry_after = bucket.retry_after();
            RateLimitResult::Denied {
                retry_after,
                limit: self.config.http_requests_per_second,
                reset_at,
            }
        }
    }

    /// Check rate limit for HTTP request using IP if no API key provided
    pub fn check_http(&self, key: Option<&str>, ip: IpAddr) -> RateLimitResult {
        match key {
            Some(k) => self.check_key(k),
            None => self.check_key(&ip.to_string()),
        }
    }

    /// Clean up stale buckets that haven't been used recently
    pub fn cleanup_stale(&self) -> usize {
        let ttl_ms = (self.config.bucket_ttl_seconds * 1000) as i64;
        let now = TokenBucket::now_millis();
        let mut removed = 0;

        // Clean IP buckets
        self.ip_buckets.retain(|_, entry| {
            let age = now - entry.bucket.last_activity();
            if age >= ttl_ms {
                removed += 1;
                false
            } else {
                true
            }
        });

        // Clean key buckets
        self.key_buckets.retain(|_, entry| {
            let age = now - entry.bucket.last_activity();
            if age >= ttl_ms {
                removed += 1;
                false
            } else {
                true
            }
        });

        if removed > 0 {
            tracing::debug!(
                removed = removed,
                ip_buckets = self.ip_buckets.len(),
                key_buckets = self.key_buckets.len(),
                "Cleaned up stale rate limit buckets"
            );
        }

        removed
    }

    /// Get statistics about the rate limiter
    pub fn stats(&self) -> RateLimiterStats {
        RateLimiterStats {
            enabled: self.config.enabled,
            ip_buckets: self.ip_buckets.len(),
            key_buckets: self.key_buckets.len(),
            http_limit: self.config.http_requests_per_second,
            ws_limit: self.config.ws_connections_per_minute,
        }
    }
}

/// Statistics about the rate limiter
#[derive(Debug, Clone, Serialize)]
pub struct RateLimiterStats {
    pub enabled: bool,
    pub ip_buckets: usize,
    pub key_buckets: usize,
    pub http_limit: u32,
    pub ws_limit: u32,
}

// ============================================================================
// Distributed Rate Limiter Backend
// ============================================================================

/// Backend type for distributed rate limiting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitBackendType {
    /// Local in-memory rate limiting (single instance)
    Local,
    /// Redis-backed distributed rate limiting (multi-instance)
    Redis,
}

/// Trait for distributed rate limiting backend
#[async_trait]
pub trait DistributedRateLimiter: Send + Sync {
    /// Get the backend type
    fn backend_type(&self) -> RateLimitBackendType;

    /// Check rate limit for an identifier
    /// Returns (is_allowed, remaining_tokens, retry_after_seconds)
    async fn check_rate_limit(
        &self,
        identifier: &str,
        limit: u32,
        window_seconds: u64,
    ) -> Result<(bool, u32, u64), RateLimitError>;

    /// Get current count for an identifier (for stats)
    async fn get_count(&self, identifier: &str, window_seconds: u64) -> Result<u32, RateLimitError>;
}

/// Error type for distributed rate limiter
#[derive(Debug, Clone)]
pub enum RateLimitError {
    /// Backend operation failed
    BackendError(String),
    /// Rate limiter is disabled
    Disabled,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendError(msg) => write!(f, "Rate limit backend error: {}", msg),
            Self::Disabled => write!(f, "Rate limiter is disabled"),
        }
    }
}

impl std::error::Error for RateLimitError {}

/// Local rate limiter adapter for distributed interface
pub struct LocalRateLimiterBackend {
    limiter: Arc<RateLimiter>,
}

impl LocalRateLimiterBackend {
    pub fn new(limiter: Arc<RateLimiter>) -> Self {
        Self { limiter }
    }
}

#[async_trait]
impl DistributedRateLimiter for LocalRateLimiterBackend {
    fn backend_type(&self) -> RateLimitBackendType {
        RateLimitBackendType::Local
    }

    async fn check_rate_limit(
        &self,
        identifier: &str,
        _limit: u32,
        _window_seconds: u64,
    ) -> Result<(bool, u32, u64), RateLimitError> {
        let result = self.limiter.check_key(identifier);
        match result {
            RateLimitResult::Allowed { remaining, .. } => Ok((true, remaining, 0)),
            RateLimitResult::Denied { retry_after, .. } => Ok((false, 0, retry_after)),
        }
    }

    async fn get_count(&self, _identifier: &str, _window_seconds: u64) -> Result<u32, RateLimitError> {
        // Local backend doesn't track exact counts
        Ok(0)
    }
}

/// Redis-backed distributed rate limiter using sliding window counter
pub struct RedisRateLimiterBackend {
    pool: Arc<RedisPool>,
    prefix: String,
    enabled: bool,
}

impl RedisRateLimiterBackend {
    pub fn new(pool: Arc<RedisPool>, prefix: String, enabled: bool) -> Self {
        Self { pool, prefix, enabled }
    }

    /// Generate Redis key for rate limit counter
    fn rate_limit_key(&self, identifier: &str, window: u64) -> String {
        let current_window = TokenBucket::now_millis() / (window as i64 * 1000);
        format!("{}:{}:{}", self.prefix, identifier, current_window)
    }
}

#[async_trait]
impl DistributedRateLimiter for RedisRateLimiterBackend {
    fn backend_type(&self) -> RateLimitBackendType {
        RateLimitBackendType::Redis
    }

    async fn check_rate_limit(
        &self,
        identifier: &str,
        limit: u32,
        window_seconds: u64,
    ) -> Result<(bool, u32, u64), RateLimitError> {
        if !self.enabled {
            return Ok((true, limit, 0));
        }

        let mut conn = self.pool.get_connection().await.map_err(|e| {
            RateLimitError::BackendError(format!("Failed to get connection: {}", e))
        })?;

        let key = self.rate_limit_key(identifier, window_seconds);

        // Use a Lua script for atomic increment + check
        // This avoids race conditions between INCR and EXPIRE
        let script = redis::Script::new(
            r#"
            local current = redis.call('INCR', KEYS[1])
            if current == 1 then
                redis.call('EXPIRE', KEYS[1], ARGV[1])
            end
            return current
            "#,
        );

        let count: u32 = script
            .key(&key)
            .arg(window_seconds)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| RateLimitError::BackendError(e.to_string()))?;

        let allowed = count <= limit;
        let remaining = if count > limit { 0 } else { limit - count };
        let retry_after = if allowed { 0 } else { window_seconds };

        tracing::debug!(
            identifier = %identifier,
            count = count,
            limit = limit,
            allowed = allowed,
            "Distributed rate limit check"
        );

        Ok((allowed, remaining, retry_after))
    }

    async fn get_count(&self, identifier: &str, window_seconds: u64) -> Result<u32, RateLimitError> {
        if !self.enabled {
            return Ok(0);
        }

        let mut conn = self.pool.get_connection().await.map_err(|e| {
            RateLimitError::BackendError(format!("Failed to get connection: {}", e))
        })?;

        let key = self.rate_limit_key(identifier, window_seconds);
        let count: Option<u32> = conn.get(&key).await
            .map_err(|e| RateLimitError::BackendError(e.to_string()))?;

        Ok(count.unwrap_or(0))
    }
}

/// Create a distributed rate limiter based on configuration
pub fn create_distributed_rate_limiter(
    config: &RateLimitConfig,
    local_limiter: Arc<RateLimiter>,
    redis_pool: Option<Arc<RedisPool>>,
) -> Arc<dyn DistributedRateLimiter> {
    if config.backend == "redis" {
        if let Some(pool) = redis_pool {
            tracing::info!(
                prefix = %config.redis_prefix,
                "Creating Redis distributed rate limiter"
            );
            Arc::new(RedisRateLimiterBackend::new(
                pool,
                config.redis_prefix.clone(),
                config.enabled,
            ))
        } else {
            tracing::warn!(
                "Redis rate limiter requested but pool not available, falling back to local"
            );
            Arc::new(LocalRateLimiterBackend::new(local_limiter))
        }
    } else {
        tracing::info!("Using local rate limiter");
        Arc::new(LocalRateLimiterBackend::new(local_limiter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    #[test]
    fn test_token_bucket_basic() {
        let bucket = TokenBucket::new(10, 10); // 10 capacity, 10/sec refill

        // Should be able to consume up to capacity
        for _ in 0..10 {
            assert!(bucket.try_consume());
        }

        // Should be empty now
        assert!(!bucket.try_consume());
    }

    #[test]
    fn test_token_bucket_refill() {
        let bucket = TokenBucket::new(5, 1000); // 5 capacity, 1000/sec refill

        // Consume all tokens
        for _ in 0..5 {
            assert!(bucket.try_consume());
        }

        // Wait a tiny bit for refill (1000/sec = 1 token per ms)
        std::thread::sleep(Duration::from_millis(10));

        // Should have refilled some tokens
        assert!(bucket.try_consume());
    }

    #[test]
    fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            enabled: false,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // Should always be allowed when disabled
        for _ in 0..100 {
            assert!(limiter.check_ip(ip).is_allowed());
        }
    }

    #[test]
    fn test_rate_limiter_ip_limit() {
        let config = RateLimitConfig {
            enabled: true,
            ws_connections_per_minute: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // First 5 should be allowed
        for _ in 0..5 {
            assert!(limiter.check_ip(ip).is_allowed());
        }

        // 6th should be denied
        assert!(!limiter.check_ip(ip).is_allowed());
    }

    #[test]
    fn test_rate_limiter_key_limit() {
        let config = RateLimitConfig {
            enabled: true,
            http_requests_per_second: 10,
            http_burst_size: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // First 5 should be allowed (burst size)
        for _ in 0..5 {
            assert!(limiter.check_key("test-api-key").is_allowed());
        }

        // 6th should be denied
        assert!(!limiter.check_key("test-api-key").is_allowed());
    }

    #[test]
    fn test_rate_limiter_different_keys() {
        let config = RateLimitConfig {
            enabled: true,
            http_burst_size: 3,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Each key has its own bucket
        for _ in 0..3 {
            assert!(limiter.check_key("key-1").is_allowed());
        }
        assert!(!limiter.check_key("key-1").is_allowed());

        // key-2 should still have its full quota
        for _ in 0..3 {
            assert!(limiter.check_key("key-2").is_allowed());
        }
        assert!(!limiter.check_key("key-2").is_allowed());
    }

    #[test]
    fn test_rate_limit_result_headers() {
        let config = RateLimitConfig {
            enabled: true,
            http_burst_size: 5,
            http_requests_per_second: 10,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        let result = limiter.check_key("test");
        match result {
            RateLimitResult::Allowed {
                remaining,
                limit,
                reset_at,
            } => {
                assert!(remaining <= 5);
                assert_eq!(limit, 10);
                assert!(reset_at > 0);
            }
            _ => panic!("Expected Allowed"),
        }
    }

    #[test]
    fn test_cleanup_stale_buckets() {
        let config = RateLimitConfig {
            enabled: true,
            bucket_ttl_seconds: 0, // Immediate expiry for testing
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Create some buckets
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        limiter.check_ip(ip);
        limiter.check_key("test-key");

        assert_eq!(limiter.ip_buckets.len(), 1);
        assert_eq!(limiter.key_buckets.len(), 1);

        // Cleanup should remove them (since TTL is 0)
        let removed = limiter.cleanup_stale();
        assert_eq!(removed, 2);
        assert_eq!(limiter.ip_buckets.len(), 0);
        assert_eq!(limiter.key_buckets.len(), 0);
    }

    #[test]
    fn test_stats() {
        let config = RateLimitConfig {
            enabled: true,
            http_requests_per_second: 100,
            ws_connections_per_minute: 20,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        limiter.check_ip(ip);
        limiter.check_key("key1");
        limiter.check_key("key2");

        let stats = limiter.stats();
        assert!(stats.enabled);
        assert_eq!(stats.ip_buckets, 1);
        assert_eq!(stats.key_buckets, 2);
        assert_eq!(stats.http_limit, 100);
        assert_eq!(stats.ws_limit, 20);
    }

    #[tokio::test]
    async fn test_local_rate_limiter_backend() {
        let config = RateLimitConfig {
            enabled: true,
            http_burst_size: 5,
            ..Default::default()
        };
        let limiter = Arc::new(RateLimiter::new(config));
        let backend = LocalRateLimiterBackend::new(limiter);

        assert_eq!(backend.backend_type(), RateLimitBackendType::Local);

        // First call should be allowed
        let (allowed, remaining, _) = backend.check_rate_limit("test", 10, 60).await.unwrap();
        assert!(allowed);
        assert!(remaining <= 5);
    }

    #[test]
    fn test_rate_limit_error_display() {
        let err = RateLimitError::BackendError("test error".to_string());
        assert!(format!("{}", err).contains("test error"));

        let err = RateLimitError::Disabled;
        assert!(format!("{}", err).contains("disabled"));
    }

    #[test]
    fn test_create_local_distributed_rate_limiter() {
        let config = RateLimitConfig {
            enabled: true,
            backend: "local".to_string(),
            ..Default::default()
        };
        let limiter = Arc::new(RateLimiter::new(config.clone()));
        let distributed = create_distributed_rate_limiter(&config, limiter, None);

        assert_eq!(distributed.backend_type(), RateLimitBackendType::Local);
    }
}

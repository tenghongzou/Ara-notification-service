//! Distributed rate limiting backend

use std::sync::Arc;

use async_trait::async_trait;
use redis::AsyncCommands;

use crate::redis::pool::RedisPool;

use super::config::RateLimitConfig;
use super::limiter::{RateLimitResult, RateLimiter};
use super::token_bucket::TokenBucket;

/// Backend type for distributed rate limiting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitBackendType {
    /// Local in-memory rate limiting (single instance)
    Local,
    /// Redis-backed distributed rate limiting (multi-instance)
    Redis,
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
    async fn get_count(
        &self,
        identifier: &str,
        window_seconds: u64,
    ) -> Result<u32, RateLimitError>;
}

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

    async fn get_count(
        &self,
        _identifier: &str,
        _window_seconds: u64,
    ) -> Result<u32, RateLimitError> {
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
        Self {
            pool,
            prefix,
            enabled,
        }
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

    async fn get_count(
        &self,
        identifier: &str,
        window_seconds: u64,
    ) -> Result<u32, RateLimitError> {
        if !self.enabled {
            return Ok(0);
        }

        let mut conn = self.pool.get_connection().await.map_err(|e| {
            RateLimitError::BackendError(format!("Failed to get connection: {}", e))
        })?;

        let key = self.rate_limit_key(identifier, window_seconds);
        let count: Option<u32> = conn
            .get(&key)
            .await
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

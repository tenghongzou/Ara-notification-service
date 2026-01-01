//! Rate limiting module using Token Bucket algorithm.
//!
//! This module provides rate limiting for both HTTP API requests and WebSocket connections
//! to protect against resource exhaustion attacks.
//!
//! Supports both local (in-memory) and distributed (Redis) backends.

mod config;
mod distributed;
mod limiter;
mod token_bucket;

pub use config::RateLimitConfig;
pub use distributed::{
    create_distributed_rate_limiter, DistributedRateLimiter, LocalRateLimiterBackend,
    RateLimitBackendType, RateLimitError, RedisRateLimiterBackend,
};
pub use limiter::{RateLimitResult, RateLimiter, RateLimiterStats};
pub use token_bucket::TokenBucket;

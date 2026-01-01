//! Redis high availability module
//!
//! Provides circuit breaker pattern and exponential backoff for Redis connections.
//!
//! # Modules
//!
//! - `CircuitBreaker`: Prevents cascading failures when Redis is unavailable
//! - `ExponentialBackoff`: Provides backoff delays for reconnection attempts
//! - `RedisHealth`: Tracks Redis connection health status
//! - `pool`: Connection pool for data persistence operations

mod backoff;
mod circuit_breaker;
mod health;
pub mod pool;

pub use backoff::{BackoffConfig, ExponentialBackoff};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerStats, CircuitState};
pub use health::{RedisHealth, RedisHealthStats, RedisHealthStatus};

/// Get current time in milliseconds since epoch
pub(crate) fn current_time_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

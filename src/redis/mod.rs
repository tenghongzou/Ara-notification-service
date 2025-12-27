//! Redis high availability module
//!
//! Provides circuit breaker pattern and exponential backoff for Redis connections.

use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU8, Ordering};
use std::time::Duration;

use rand::Rng;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    /// Circuit is closed, requests flow through normally
    Closed = 0,
    /// Circuit is open, requests are rejected
    Open = 1,
    /// Circuit is half-open, allowing test requests
    HalfOpen = 2,
}

impl From<u8> for CircuitState {
    fn from(value: u8) -> Self {
        match value {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Number of successes in half-open state before closing
    pub success_threshold: u32,
    /// Time to wait before transitioning from open to half-open (ms)
    pub reset_timeout_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            reset_timeout_ms: 30_000, // 30 seconds
        }
    }
}

/// Circuit breaker for Redis connections
///
/// Implements the circuit breaker pattern to prevent cascading failures
/// when Redis is unavailable.
pub struct CircuitBreaker {
    /// Current state (0=Closed, 1=Open, 2=HalfOpen)
    state: AtomicU8,
    /// Consecutive failure count
    failure_count: AtomicU32,
    /// Consecutive success count (in half-open state)
    success_count: AtomicU32,
    /// Timestamp of last state change (ms since epoch)
    last_state_change: AtomicI64,
    /// Configuration
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with default configuration
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// Create a new circuit breaker with custom configuration
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_state_change: AtomicI64::new(current_time_ms()),
            config,
        }
    }

    /// Get the current state
    pub fn state(&self) -> CircuitState {
        self.check_state_transition();
        CircuitState::from(self.state.load(Ordering::Acquire))
    }

    /// Check if requests should be allowed
    pub fn allow_request(&self) -> bool {
        match self.state() {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => true, // Allow test requests
        }
    }

    /// Record a successful operation
    pub fn record_success(&self) {
        let state = CircuitState::from(self.state.load(Ordering::Acquire));

        match state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Release);
            }
            CircuitState::HalfOpen => {
                let success_count = self.success_count.fetch_add(1, Ordering::AcqRel) + 1;
                if success_count >= self.config.success_threshold {
                    self.transition_to(CircuitState::Closed);
                    tracing::info!("Circuit breaker closed after successful recovery");
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but ignore
            }
        }
    }

    /// Record a failed operation
    pub fn record_failure(&self) {
        let state = CircuitState::from(self.state.load(Ordering::Acquire));

        match state {
            CircuitState::Closed => {
                let failure_count = self.failure_count.fetch_add(1, Ordering::AcqRel) + 1;
                if failure_count >= self.config.failure_threshold {
                    self.transition_to(CircuitState::Open);
                    tracing::warn!(
                        failures = failure_count,
                        "Circuit breaker opened due to failures"
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open state reopens the circuit
                self.transition_to(CircuitState::Open);
                tracing::warn!("Circuit breaker reopened after failure in half-open state");
            }
            CircuitState::Open => {
                // Already open, just update timestamp
                self.last_state_change.store(current_time_ms(), Ordering::Release);
            }
        }
    }

    /// Check if we should transition from Open to HalfOpen
    fn check_state_transition(&self) {
        let state = CircuitState::from(self.state.load(Ordering::Acquire));

        if state == CircuitState::Open {
            let last_change = self.last_state_change.load(Ordering::Acquire);
            let elapsed = current_time_ms() - last_change;

            if elapsed >= self.config.reset_timeout_ms as i64 {
                // Try to transition to half-open
                if self.state.compare_exchange(
                    CircuitState::Open as u8,
                    CircuitState::HalfOpen as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ).is_ok() {
                    self.success_count.store(0, Ordering::Release);
                    self.last_state_change.store(current_time_ms(), Ordering::Release);
                    tracing::info!("Circuit breaker transitioning to half-open state");
                }
            }
        }
    }

    /// Transition to a new state
    fn transition_to(&self, new_state: CircuitState) {
        self.state.store(new_state as u8, Ordering::Release);
        self.last_state_change.store(current_time_ms(), Ordering::Release);

        match new_state {
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Release);
                self.success_count.store(0, Ordering::Release);
            }
            CircuitState::Open => {
                self.success_count.store(0, Ordering::Release);
            }
            CircuitState::HalfOpen => {
                self.success_count.store(0, Ordering::Release);
            }
        }
    }

    /// Get statistics snapshot
    pub fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: self.state(),
            failure_count: self.failure_count.load(Ordering::Acquire),
            success_count: self.success_count.load(Ordering::Acquire),
            last_state_change_ms: self.last_state_change.load(Ordering::Acquire),
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_state_change_ms: i64,
}

/// Exponential backoff configuration
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Initial delay in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Multiplier for exponential growth
    pub multiplier: f64,
    /// Jitter factor (0.0 to 1.0)
    pub jitter_factor: f64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: 100,
            max_delay_ms: 30_000, // 30 seconds
            multiplier: 2.0,
            jitter_factor: 0.1, // 10% jitter
        }
    }
}

/// Exponential backoff calculator with jitter
pub struct ExponentialBackoff {
    config: BackoffConfig,
    current_delay_ms: u64,
    attempt: u32,
}

impl ExponentialBackoff {
    /// Create a new exponential backoff with default configuration
    pub fn new() -> Self {
        Self::with_config(BackoffConfig::default())
    }

    /// Create a new exponential backoff with custom configuration
    pub fn with_config(config: BackoffConfig) -> Self {
        let initial = config.initial_delay_ms;
        Self {
            config,
            current_delay_ms: initial,
            attempt: 0,
        }
    }

    /// Get the next delay duration
    pub fn next_delay(&mut self) -> Duration {
        self.attempt += 1;

        // Calculate base delay with exponential growth
        let base_delay = self.current_delay_ms as f64 * self.config.multiplier;
        let capped_delay = base_delay.min(self.config.max_delay_ms as f64);

        // Apply jitter only if jitter_factor > 0
        let final_delay = if self.config.jitter_factor > 0.0 {
            let jitter_range = capped_delay * self.config.jitter_factor;
            let jitter = rand::thread_rng().gen_range(-jitter_range..jitter_range);
            (capped_delay + jitter).max(1.0) as u64
        } else {
            capped_delay.max(1.0) as u64
        };

        self.current_delay_ms = final_delay;

        Duration::from_millis(final_delay)
    }

    /// Reset the backoff to initial state
    pub fn reset(&mut self) {
        self.current_delay_ms = self.config.initial_delay_ms;
        self.attempt = 0;
    }

    /// Get the current attempt number
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self::new()
    }
}

/// Redis connection health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedisHealthStatus {
    /// Redis is connected and healthy
    Healthy,
    /// Redis is disconnected, attempting to reconnect
    Reconnecting,
    /// Circuit breaker is open, not attempting connections
    CircuitOpen,
}

impl RedisHealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RedisHealthStatus::Healthy => "healthy",
            RedisHealthStatus::Reconnecting => "reconnecting",
            RedisHealthStatus::CircuitOpen => "circuit_open",
        }
    }
}

/// Redis health tracker
pub struct RedisHealth {
    status: AtomicU8,
    last_connected: AtomicI64,
    reconnection_attempts: AtomicU32,
    total_reconnections: AtomicU32,
}

impl RedisHealth {
    pub fn new() -> Self {
        Self {
            status: AtomicU8::new(RedisHealthStatus::Reconnecting as u8),
            last_connected: AtomicI64::new(0),
            reconnection_attempts: AtomicU32::new(0),
            total_reconnections: AtomicU32::new(0),
        }
    }

    /// Mark Redis as connected
    pub fn set_connected(&self) {
        let was_reconnecting = self.status.load(Ordering::Acquire) != RedisHealthStatus::Healthy as u8;
        self.status.store(RedisHealthStatus::Healthy as u8, Ordering::Release);
        self.last_connected.store(current_time_ms(), Ordering::Release);

        if was_reconnecting {
            self.total_reconnections.fetch_add(1, Ordering::AcqRel);
        }
        self.reconnection_attempts.store(0, Ordering::Release);
    }

    /// Mark Redis as reconnecting
    pub fn set_reconnecting(&self) {
        self.status.store(RedisHealthStatus::Reconnecting as u8, Ordering::Release);
        self.reconnection_attempts.fetch_add(1, Ordering::AcqRel);
    }

    /// Mark circuit as open
    pub fn set_circuit_open(&self) {
        self.status.store(RedisHealthStatus::CircuitOpen as u8, Ordering::Release);
    }

    /// Get current status
    pub fn status(&self) -> RedisHealthStatus {
        match self.status.load(Ordering::Acquire) {
            0 => RedisHealthStatus::Healthy,
            1 => RedisHealthStatus::Reconnecting,
            2 => RedisHealthStatus::CircuitOpen,
            _ => RedisHealthStatus::Reconnecting,
        }
    }

    /// Check if Redis is healthy
    pub fn is_healthy(&self) -> bool {
        self.status() == RedisHealthStatus::Healthy
    }

    /// Get statistics snapshot
    pub fn stats(&self) -> RedisHealthStats {
        RedisHealthStats {
            status: self.status(),
            last_connected_ms: self.last_connected.load(Ordering::Acquire),
            reconnection_attempts: self.reconnection_attempts.load(Ordering::Acquire),
            total_reconnections: self.total_reconnections.load(Ordering::Acquire),
        }
    }
}

impl Default for RedisHealth {
    fn default() -> Self {
        Self::new()
    }
}

/// Redis health statistics
#[derive(Debug, Clone)]
pub struct RedisHealthStats {
    pub status: RedisHealthStatus,
    pub last_connected_ms: i64,
    pub reconnection_attempts: u32,
    pub total_reconnections: u32,
}

/// Get current time in milliseconds since epoch
fn current_time_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_initial_state() {
        let cb = CircuitBreaker::new();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            reset_timeout_ms: 1000,
        };
        let cb = CircuitBreaker::with_config(config);

        // Record failures
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure(); // 3rd failure
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_success_resets_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            reset_timeout_ms: 1000,
        };
        let cb = CircuitBreaker::with_config(config);

        cb.record_failure();
        cb.record_failure();
        cb.record_success(); // Reset failures

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed); // Still closed, need 3 consecutive
    }

    #[test]
    fn test_circuit_breaker_half_open_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            reset_timeout_ms: 50, // Short timeout for testing
        };
        let cb = CircuitBreaker::with_config(config);

        cb.record_failure();
        // Immediately after failure, should be open (before timeout)
        let state_raw = CircuitState::from(cb.state.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(state_raw, CircuitState::Open);

        // Wait for timeout to pass
        std::thread::sleep(Duration::from_millis(60));
        // Now state() should transition to half-open
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_closes_after_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            reset_timeout_ms: 10,
        };
        let cb = CircuitBreaker::with_config(config);

        cb.record_failure();
        std::thread::sleep(Duration::from_millis(20));
        let _ = cb.state(); // Trigger transition to half-open

        cb.record_success();
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_success(); // 2nd success
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_reopens_on_half_open_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            reset_timeout_ms: 10,
        };
        let cb = CircuitBreaker::with_config(config);

        cb.record_failure();
        std::thread::sleep(Duration::from_millis(20));
        let _ = cb.state(); // Transition to half-open
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_failure(); // Failure in half-open should reopen
        // After failure in half-open, should transition back to open
        let state_raw = CircuitState::from(cb.state.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(state_raw, CircuitState::Open);
    }

    #[test]
    fn test_exponential_backoff_increases() {
        let config = BackoffConfig {
            initial_delay_ms: 100,
            max_delay_ms: 10000,
            multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable testing
        };
        let mut backoff = ExponentialBackoff::with_config(config);

        let d1 = backoff.next_delay();
        let d2 = backoff.next_delay();
        let d3 = backoff.next_delay();

        // Each delay should roughly double (within rounding)
        assert!(d2 > d1);
        assert!(d3 > d2);
    }

    #[test]
    fn test_exponential_backoff_caps_at_max() {
        let config = BackoffConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 5000,
            multiplier: 10.0,
            jitter_factor: 0.0,
        };
        let mut backoff = ExponentialBackoff::with_config(config);

        // Should hit max quickly
        for _ in 0..5 {
            backoff.next_delay();
        }

        let delay = backoff.next_delay();
        assert!(delay.as_millis() <= 5000);
    }

    #[test]
    fn test_exponential_backoff_reset() {
        let config = BackoffConfig {
            initial_delay_ms: 100,
            max_delay_ms: 10000,
            multiplier: 2.0,
            jitter_factor: 0.0,
        };
        let mut backoff = ExponentialBackoff::with_config(config);

        backoff.next_delay();
        backoff.next_delay();
        backoff.next_delay();

        backoff.reset();
        assert_eq!(backoff.attempt(), 0);
    }

    #[test]
    fn test_redis_health_status() {
        let health = RedisHealth::new();
        assert_eq!(health.status(), RedisHealthStatus::Reconnecting);
        assert!(!health.is_healthy());

        health.set_connected();
        assert_eq!(health.status(), RedisHealthStatus::Healthy);
        assert!(health.is_healthy());

        health.set_reconnecting();
        assert_eq!(health.status(), RedisHealthStatus::Reconnecting);
    }

    #[test]
    fn test_redis_health_stats() {
        let health = RedisHealth::new();

        health.set_reconnecting();
        health.set_reconnecting();
        health.set_connected();

        let stats = health.stats();
        assert_eq!(stats.status, RedisHealthStatus::Healthy);
        assert_eq!(stats.total_reconnections, 1);
        assert_eq!(stats.reconnection_attempts, 0); // Reset on connect
    }
}

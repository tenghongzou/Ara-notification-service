//! Circuit breaker pattern implementation for Redis connections

use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU8, Ordering};

use super::current_time_ms;

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
                self.last_state_change
                    .store(current_time_ms(), Ordering::Release);
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
                if self
                    .state
                    .compare_exchange(
                        CircuitState::Open as u8,
                        CircuitState::HalfOpen as u8,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    self.success_count.store(0, Ordering::Release);
                    self.last_state_change
                        .store(current_time_ms(), Ordering::Release);
                    tracing::info!("Circuit breaker transitioning to half-open state");
                }
            }
        }
    }

    /// Transition to a new state
    fn transition_to(&self, new_state: CircuitState) {
        self.state.store(new_state as u8, Ordering::Release);
        self.last_state_change
            .store(current_time_ms(), Ordering::Release);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
}

//! Exponential backoff implementation for Redis reconnection

use std::time::Duration;

use rand::Rng;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}

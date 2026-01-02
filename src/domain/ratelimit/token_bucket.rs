//! Token Bucket algorithm implementation

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::time::SystemTime;

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
    pub fn now_millis() -> i64 {
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

#[cfg(test)]
mod tests {
    use super::*;
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
}

//! Redis connection health tracking

use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU8, Ordering};

use super::current_time_ms;

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
        let was_reconnecting =
            self.status.load(Ordering::Acquire) != RedisHealthStatus::Healthy as u8;
        self.status
            .store(RedisHealthStatus::Healthy as u8, Ordering::Release);
        self.last_connected
            .store(current_time_ms(), Ordering::Release);

        if was_reconnecting {
            self.total_reconnections.fetch_add(1, Ordering::AcqRel);
        }
        self.reconnection_attempts.store(0, Ordering::Release);
    }

    /// Mark Redis as reconnecting
    pub fn set_reconnecting(&self) {
        self.status
            .store(RedisHealthStatus::Reconnecting as u8, Ordering::Release);
        self.reconnection_attempts.fetch_add(1, Ordering::AcqRel);
    }

    /// Mark circuit as open
    pub fn set_circuit_open(&self) {
        self.status
            .store(RedisHealthStatus::CircuitOpen as u8, Ordering::Release);
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

#[cfg(test)]
mod tests {
    use super::*;

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

//! Redis connection pool for persistent storage backends.
//!
//! Provides a managed Redis connection pool with circuit breaker
//! integration for resilient data operations.

use std::sync::Arc;

use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client, RedisError, RedisResult};
use tokio::sync::RwLock;

use crate::config::RedisConfig;

use super::{CircuitBreaker, CircuitState, RedisHealth};

/// Error type for Redis pool operations.
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// Redis operation failed
    #[error("Redis error: {0}")]
    Redis(#[from] RedisError),

    /// Circuit breaker is open
    #[error("Circuit breaker is open")]
    CircuitOpen,

    /// Connection not available
    #[error("Connection not available: {0}")]
    ConnectionUnavailable(String),
}

/// Redis connection pool for data operations.
///
/// This pool manages a multiplexed Redis connection and integrates with
/// the circuit breaker for fault tolerance. It's designed for use by
/// persistence backends (queue, ACK tracking, etc.).
///
/// Unlike the Pub/Sub subscriber which uses a dedicated connection,
/// this pool uses multiplexed connections suitable for commands.
pub struct RedisPool {
    /// Redis client for creating connections
    client: Client,

    /// Multiplexed connection (shared across tasks)
    connection: RwLock<Option<MultiplexedConnection>>,

    /// Circuit breaker for fault tolerance
    circuit_breaker: Arc<CircuitBreaker>,

    /// Health tracker
    health: Arc<RedisHealth>,

    /// Configuration
    config: RedisConfig,
}

impl RedisPool {
    /// Create a new Redis pool.
    pub fn new(
        config: RedisConfig,
        circuit_breaker: Arc<CircuitBreaker>,
        health: Arc<RedisHealth>,
    ) -> Result<Self, PoolError> {
        let client = Client::open(config.url.as_str())?;

        Ok(Self {
            client,
            connection: RwLock::new(None),
            circuit_breaker,
            health,
            config,
        })
    }

    /// Get a connection from the pool.
    ///
    /// This will establish a new connection if none exists.
    /// Returns an error if the circuit breaker is open.
    pub async fn get_connection(&self) -> Result<MultiplexedConnection, PoolError> {
        // Check circuit breaker first
        if !self.circuit_breaker.allow_request() {
            self.health.set_circuit_open();
            return Err(PoolError::CircuitOpen);
        }

        // Try to get existing connection
        {
            let conn = self.connection.read().await;
            if let Some(ref c) = *conn {
                return Ok(c.clone());
            }
        }

        // Need to create new connection
        self.connect().await
    }

    /// Establish a new connection.
    async fn connect(&self) -> Result<MultiplexedConnection, PoolError> {
        let mut conn_guard = self.connection.write().await;

        // Double-check in case another task connected while we waited
        if let Some(ref c) = *conn_guard {
            return Ok(c.clone());
        }

        self.health.set_reconnecting();

        match self.client.get_multiplexed_tokio_connection().await {
            Ok(conn) => {
                *conn_guard = Some(conn.clone());
                self.circuit_breaker.record_success();
                self.health.set_connected();
                tracing::info!("Redis pool connection established");
                Ok(conn)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                tracing::error!(error = %e, "Failed to connect to Redis");
                Err(PoolError::Redis(e))
            }
        }
    }

    /// Execute a Redis command with circuit breaker protection.
    ///
    /// This is a helper that handles connection management and
    /// circuit breaker recording automatically.
    pub async fn execute<F, T, Fut>(&self, f: F) -> Result<T, PoolError>
    where
        F: FnOnce(MultiplexedConnection) -> Fut,
        Fut: std::future::Future<Output = RedisResult<T>>,
    {
        let conn = self.get_connection().await?;

        match f(conn).await {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(e) => {
                // Check if this is a connection error that should trigger reconnect
                if e.is_connection_dropped() || e.is_io_error() {
                    // Clear the connection so next call will reconnect
                    let mut conn_guard = self.connection.write().await;
                    *conn_guard = None;
                }
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    /// Check if the pool is healthy (circuit breaker closed and connected).
    pub fn is_healthy(&self) -> bool {
        self.health.is_healthy() && self.circuit_breaker.state() == CircuitState::Closed
    }

    /// Get the circuit breaker state.
    pub fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state()
    }

    /// Get the Redis URL (for debugging).
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Ping Redis to check connectivity.
    pub async fn ping(&self) -> Result<(), PoolError> {
        let mut conn = self.get_connection().await?;
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        Ok(())
    }
}

/// Extension trait for common Redis operations.
///
/// Provides typed helper methods for common Redis commands used by
/// the persistence backends.
#[async_trait::async_trait]
pub trait RedisPoolExt {
    // Stream operations (for queue)

    /// Add entry to a stream with MAXLEN trimming.
    async fn xadd_maxlen(
        &self,
        key: &str,
        maxlen: usize,
        fields: &[(&str, &str)],
    ) -> Result<String, PoolError>;

    /// Read all entries from a stream.
    async fn xrange_all(&self, key: &str) -> Result<Vec<(String, Vec<(String, String)>)>, PoolError>;

    /// Delete a stream key.
    async fn del(&self, key: &str) -> Result<(), PoolError>;

    /// Check if a key exists.
    async fn exists(&self, key: &str) -> Result<bool, PoolError>;

    // Hash operations (for ACK tracking)

    /// Set multiple hash fields.
    async fn hset_multiple(&self, key: &str, fields: &[(&str, &str)]) -> Result<(), PoolError>;

    /// Get a hash field.
    async fn hget(&self, key: &str, field: &str) -> Result<Option<String>, PoolError>;

    /// Get all hash fields and values.
    async fn hgetall(&self, key: &str) -> Result<Vec<(String, String)>, PoolError>;

    /// Delete a hash key.
    async fn hdel(&self, key: &str) -> Result<(), PoolError>;

    /// Increment a hash field by value.
    async fn hincrby(&self, key: &str, field: &str, increment: i64) -> Result<i64, PoolError>;

    // Sorted set operations (for timeout tracking)

    /// Add to sorted set.
    async fn zadd(&self, key: &str, score: f64, member: &str) -> Result<(), PoolError>;

    /// Remove from sorted set.
    async fn zrem(&self, key: &str, member: &str) -> Result<(), PoolError>;

    /// Get members with scores less than max.
    async fn zrangebyscore(&self, key: &str, min: f64, max: f64) -> Result<Vec<String>, PoolError>;

    // Key operations

    /// Set key expiration.
    async fn expire(&self, key: &str, seconds: i64) -> Result<(), PoolError>;
}

#[async_trait::async_trait]
impl RedisPoolExt for RedisPool {
    async fn xadd_maxlen(
        &self,
        key: &str,
        maxlen: usize,
        fields: &[(&str, &str)],
    ) -> Result<String, PoolError> {
        let mut conn = self.get_connection().await?;

        // Build XADD command: XADD key MAXLEN ~ maxlen * field value ...
        let mut cmd = redis::cmd("XADD");
        cmd.arg(key)
            .arg("MAXLEN")
            .arg("~")
            .arg(maxlen)
            .arg("*");

        for (field, value) in fields {
            cmd.arg(*field).arg(*value);
        }

        match cmd.query_async(&mut conn).await {
            Ok(id) => {
                self.circuit_breaker.record_success();
                Ok(id)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn xrange_all(&self, key: &str) -> Result<Vec<(String, Vec<(String, String)>)>, PoolError> {
        let mut conn = self.get_connection().await?;

        match redis::cmd("XRANGE")
            .arg(key)
            .arg("-")
            .arg("+")
            .query_async(&mut conn)
            .await
        {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn del(&self, key: &str) -> Result<(), PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.del::<_, ()>(key).await {
            Ok(_) => {
                self.circuit_breaker.record_success();
                Ok(())
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.exists(key).await {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn hset_multiple(&self, key: &str, fields: &[(&str, &str)]) -> Result<(), PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.hset_multiple::<_, _, _, ()>(key, fields).await {
            Ok(_) => {
                self.circuit_breaker.record_success();
                Ok(())
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn hget(&self, key: &str, field: &str) -> Result<Option<String>, PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.hget(key, field).await {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn hgetall(&self, key: &str) -> Result<Vec<(String, String)>, PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.hgetall(key).await {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn hdel(&self, key: &str) -> Result<(), PoolError> {
        self.del(key).await
    }

    async fn hincrby(&self, key: &str, field: &str, increment: i64) -> Result<i64, PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.hincr(key, field, increment).await {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn zadd(&self, key: &str, score: f64, member: &str) -> Result<(), PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.zadd::<_, _, _, ()>(key, member, score).await {
            Ok(_) => {
                self.circuit_breaker.record_success();
                Ok(())
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn zrem(&self, key: &str, member: &str) -> Result<(), PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.zrem::<_, _, ()>(key, member).await {
            Ok(_) => {
                self.circuit_breaker.record_success();
                Ok(())
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn zrangebyscore(&self, key: &str, min: f64, max: f64) -> Result<Vec<String>, PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.zrangebyscore(key, min, max).await {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }

    async fn expire(&self, key: &str, seconds: i64) -> Result<(), PoolError> {
        let mut conn = self.get_connection().await?;

        match conn.expire::<_, ()>(key, seconds).await {
            Ok(_) => {
                self.circuit_breaker.record_success();
                Ok(())
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PoolError::Redis(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> RedisConfig {
        RedisConfig {
            url: "redis://localhost:6379".to_string(),
            channels: vec![],
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_success_threshold: 2,
            circuit_breaker_reset_timeout_seconds: 30,
            backoff_initial_delay_ms: 100,
            backoff_max_delay_ms: 30000,
        }
    }

    #[test]
    fn test_pool_creation() {
        let config = create_test_config();
        let cb = Arc::new(CircuitBreaker::new());
        let health = Arc::new(RedisHealth::new());

        let pool = RedisPool::new(config, cb.clone(), health.clone());
        assert!(pool.is_ok());

        let pool = pool.unwrap();
        assert_eq!(pool.url(), "redis://localhost:6379");
    }

    #[test]
    fn test_pool_circuit_breaker_integration() {
        let config = create_test_config();
        let cb = Arc::new(CircuitBreaker::new());
        let health = Arc::new(RedisHealth::new());

        let pool = RedisPool::new(config, cb.clone(), health.clone()).unwrap();

        // Initially circuit should be closed
        assert_eq!(pool.circuit_state(), CircuitState::Closed);

        // Record failures to open circuit
        for _ in 0..5 {
            cb.record_failure();
        }

        assert_eq!(pool.circuit_state(), CircuitState::Open);
        assert!(!pool.is_healthy());
    }

    #[test]
    fn test_pool_error_display() {
        let circuit_err = PoolError::CircuitOpen;
        assert_eq!(format!("{}", circuit_err), "Circuit breaker is open");

        let unavail_err = PoolError::ConnectionUnavailable("test".to_string());
        assert!(format!("{}", unavail_err).contains("test"));
    }
}

//! PostgreSQL connection pool with circuit breaker integration.

use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;

use crate::config::DatabaseConfig;
use crate::redis::CircuitBreaker;

/// Errors that can occur with the PostgreSQL pool.
#[derive(Debug, Error)]
pub enum PostgresPoolError {
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Circuit breaker is open")]
    CircuitOpen,

    #[error("Connection unavailable: {0}")]
    ConnectionUnavailable(String),
}

/// PostgreSQL connection pool with circuit breaker integration.
pub struct PostgresPool {
    /// The underlying connection pool
    pool: PgPool,

    /// Circuit breaker for connection failures
    circuit_breaker: Arc<CircuitBreaker>,

    /// Database URL (for logging purposes)
    database_url: String,
}

impl PostgresPool {
    /// Create a new PostgreSQL pool from configuration.
    pub async fn new(
        config: &DatabaseConfig,
        circuit_breaker: Arc<CircuitBreaker>,
    ) -> Result<Self, PostgresPoolError> {
        let pool = PgPoolOptions::new()
            .max_connections(config.pool_size)
            .acquire_timeout(Duration::from_secs(config.connect_timeout_seconds as u64))
            .idle_timeout(Duration::from_secs(config.idle_timeout_seconds as u64))
            .connect(&config.url)
            .await?;

        tracing::info!(
            pool_size = config.pool_size,
            "PostgreSQL connection pool created"
        );

        Ok(Self {
            pool,
            circuit_breaker,
            database_url: config.url.clone(),
        })
    }

    /// Get a reference to the underlying pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get the circuit breaker.
    pub fn circuit_breaker(&self) -> &Arc<CircuitBreaker> {
        &self.circuit_breaker
    }

    /// Check if the circuit breaker allows operations.
    pub fn is_available(&self) -> bool {
        self.circuit_breaker.allow_request()
    }

    /// Record a successful operation.
    pub fn record_success(&self) {
        self.circuit_breaker.record_success();
    }

    /// Record a failed operation.
    pub fn record_failure(&self) {
        self.circuit_breaker.record_failure();
    }

    /// Execute an operation with circuit breaker protection.
    pub async fn execute_with_circuit_breaker<T, F, Fut>(
        &self,
        operation: F,
    ) -> Result<T, PostgresPoolError>
    where
        F: FnOnce(&PgPool) -> Fut,
        Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
    {
        if !self.circuit_breaker.allow_request() {
            return Err(PostgresPoolError::CircuitOpen);
        }

        match operation(&self.pool).await {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(PostgresPoolError::Sqlx(e))
            }
        }
    }

    /// Get the database URL (masked for logging).
    pub fn database_url_masked(&self) -> String {
        // Mask password in URL for safe logging
        if let Some(at_pos) = self.database_url.find('@') {
            if let Some(colon_pos) = self.database_url[..at_pos].rfind(':') {
                let prefix = &self.database_url[..colon_pos + 1];
                let suffix = &self.database_url[at_pos..];
                return format!("{}***{}", prefix, suffix);
            }
        }
        self.database_url.clone()
    }

    /// Close the pool gracefully.
    pub async fn close(&self) {
        self.pool.close().await;
        tracing::info!("PostgreSQL connection pool closed");
    }
}

impl Clone for PostgresPool {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            circuit_breaker: self.circuit_breaker.clone(),
            database_url: self.database_url.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_masking_logic() {
        // Test URL masking logic without needing a real pool
        fn mask_url(url: &str) -> String {
            if let Some(at_pos) = url.find('@') {
                if let Some(colon_pos) = url[..at_pos].rfind(':') {
                    let prefix = &url[..colon_pos + 1];
                    let suffix = &url[at_pos..];
                    return format!("{}***{}", prefix, suffix);
                }
            }
            url.to_string()
        }

        // Test with password
        let url = "postgres://user:secret123@localhost:5432/db";
        let masked = mask_url(url);
        assert!(masked.contains("***"));
        assert!(!masked.contains("secret123"));
        assert!(masked.contains("user:"));
        assert!(masked.contains("@localhost:5432"));

        // Test without password (just host)
        let url_no_pass = "postgres://localhost:5432/db";
        let masked_no_pass = mask_url(url_no_pass);
        assert_eq!(masked_no_pass, url_no_pass);
    }

    #[test]
    fn test_error_types() {
        // Test error creation
        let err = PostgresPoolError::CircuitOpen;
        assert!(format!("{}", err).contains("Circuit breaker is open"));

        let err = PostgresPoolError::ConnectionUnavailable("test error".to_string());
        assert!(format!("{}", err).contains("test error"));
    }
}

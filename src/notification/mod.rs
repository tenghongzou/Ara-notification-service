//! Notification types, dispatching, and tracking.
//!
//! # ACK Backend Architecture
//!
//! The ACK tracking system uses a backend abstraction to support different storage
//! implementations:
//!
//! - `MemoryAckBackend`: In-memory storage using DashMap (default)
//! - `RedisAckBackend`: Persistent storage using Redis Hash + Sorted Set
//! - `PostgresAckBackend`: Persistent storage using PostgreSQL
//!
//! Use `create_ack_backend()` to create the appropriate backend based on configuration.

pub mod ack;
pub mod ack_backend;
pub mod ack_memory_backend;
pub mod ack_postgres_backend;
pub mod ack_redis_backend;
mod dispatcher;
mod types;

use std::sync::Arc;

use crate::config::AckSettingsConfig;
use crate::redis::pool::RedisPool;

pub use ack::{AckConfig, AckStatsSnapshot, AckTracker};
pub use ack_backend::{AckBackendError, AckBackendStats, AckTrackerBackend, PendingAckInfo};
pub use ack_memory_backend::MemoryAckBackend;
pub use ack_postgres_backend::PostgresAckBackend;
pub use ack_redis_backend::RedisAckBackend;
pub use dispatcher::{DeliveryResult, NotificationDispatcher};
pub use types::{
    Audience, NotificationBuilder, NotificationEvent, NotificationMetadata, NotificationTarget,
    Priority,
};

use crate::postgres::PostgresPool;

/// Create an ACK tracking backend based on configuration.
///
/// Returns the appropriate backend implementation based on the `backend` setting:
/// - `"postgres"`: Returns a `PostgresAckBackend` if a PostgreSQL pool is provided
/// - `"redis"`: Returns a `RedisAckBackend` if a Redis pool is provided
/// - `"memory"` (default): Returns a `MemoryAckBackend`
///
/// # Arguments
///
/// * `settings` - ACK configuration from settings
/// * `redis_pool` - Optional Redis connection pool (required for Redis backend)
/// * `postgres_pool` - Optional PostgreSQL connection pool (required for Postgres backend)
/// * `tenant_id` - Tenant ID for multi-tenant isolation (defaults to "default")
///
/// # Example
///
/// ```rust,ignore
/// let backend = create_ack_backend(&settings.ack, Some(redis_pool.clone()), Some(pg_pool.clone()), None);
/// ```
pub fn create_ack_backend(
    settings: &AckSettingsConfig,
    redis_pool: Option<Arc<RedisPool>>,
    postgres_pool: Option<Arc<PostgresPool>>,
    tenant_id: Option<String>,
) -> Arc<dyn AckTrackerBackend> {
    let config = AckConfig {
        enabled: settings.enabled,
        timeout_seconds: settings.timeout_seconds,
        cleanup_interval_seconds: settings.cleanup_interval_seconds,
    };

    match settings.backend.as_str() {
        "postgres" => {
            if let Some(pool) = postgres_pool {
                let tenant = tenant_id.unwrap_or_else(|| "default".to_string());
                tracing::info!(
                    backend = "postgres",
                    tenant_id = %tenant,
                    "Creating PostgreSQL ACK backend"
                );
                Arc::new(PostgresAckBackend::with_tenant(
                    config,
                    pool.pool().clone(),
                    tenant,
                ))
            } else {
                tracing::warn!(
                    "PostgreSQL ACK backend requested but no pool provided, falling back to memory"
                );
                Arc::new(MemoryAckBackend::new(config))
            }
        }
        "redis" => {
            if let Some(pool) = redis_pool {
                let tenant = tenant_id.clone().unwrap_or_else(|| "default".to_string());
                tracing::info!(
                    backend = "redis",
                    prefix = %settings.redis_prefix,
                    tenant_id = %tenant,
                    "Creating Redis ACK backend"
                );
                Arc::new(RedisAckBackend::with_tenant(
                    config,
                    pool,
                    settings.redis_prefix.clone(),
                    tenant,
                ))
            } else {
                tracing::warn!(
                    "Redis ACK backend requested but no pool provided, falling back to memory"
                );
                Arc::new(MemoryAckBackend::new(config))
            }
        }
        _ => {
            tracing::info!(backend = "memory", "Creating memory ACK backend");
            Arc::new(MemoryAckBackend::new(config))
        }
    }
}

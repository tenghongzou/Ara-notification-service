//! Queue backend factory

use std::sync::Arc;

use crate::config::QueueConfig as SettingsQueueConfig;
use crate::postgres::PostgresPool;
use crate::redis::pool::RedisPool;

use super::backend::MessageQueueBackend;
use super::memory_backend::MemoryQueueBackend;
use super::models::QueueConfig;
use super::postgres_backend::PostgresQueueBackend;
use super::redis_backend::RedisQueueBackend;

/// Create a queue backend based on configuration.
///
/// Returns the appropriate backend implementation based on the `backend` setting:
/// - `"postgres"`: Returns a `PostgresQueueBackend` if a PostgreSQL pool is provided
/// - `"redis"`: Returns a `RedisQueueBackend` if a Redis pool is provided
/// - `"memory"` (default): Returns a `MemoryQueueBackend`
///
/// # Arguments
///
/// * `settings` - Queue configuration from settings
/// * `redis_pool` - Optional Redis connection pool (required for Redis backend)
/// * `postgres_pool` - Optional PostgreSQL connection pool (required for Postgres backend)
/// * `tenant_id` - Tenant ID for multi-tenant isolation (defaults to "default")
///
/// # Example
///
/// ```rust,ignore
/// let backend = create_queue_backend(&settings.queue, Some(redis_pool.clone()), Some(pg_pool.clone()), None);
/// ```
pub fn create_queue_backend(
    settings: &SettingsQueueConfig,
    redis_pool: Option<Arc<RedisPool>>,
    postgres_pool: Option<Arc<PostgresPool>>,
    tenant_id: Option<String>,
) -> Arc<dyn MessageQueueBackend> {
    let config = QueueConfig {
        enabled: settings.enabled,
        max_queue_size_per_user: settings.max_size_per_user,
        message_ttl_seconds: settings.message_ttl_seconds,
        cleanup_interval_seconds: settings.cleanup_interval_seconds,
    };

    match settings.backend.as_str() {
        "postgres" => {
            if let Some(pool) = postgres_pool {
                let tenant = tenant_id.unwrap_or_else(|| "default".to_string());
                tracing::info!(
                    backend = "postgres",
                    tenant_id = %tenant,
                    "Creating PostgreSQL queue backend"
                );
                Arc::new(PostgresQueueBackend::with_tenant(
                    config,
                    pool.pool().clone(),
                    tenant,
                ))
            } else {
                tracing::warn!(
                    "PostgreSQL backend requested but no pool provided, falling back to memory"
                );
                Arc::new(MemoryQueueBackend::new(config))
            }
        }
        "redis" => {
            if let Some(pool) = redis_pool {
                let tenant = tenant_id.clone().unwrap_or_else(|| "default".to_string());
                tracing::info!(
                    backend = "redis",
                    prefix = %settings.redis_prefix,
                    tenant_id = %tenant,
                    "Creating Redis queue backend"
                );
                Arc::new(RedisQueueBackend::with_tenant(
                    config,
                    pool,
                    settings.redis_prefix.clone(),
                    tenant,
                ))
            } else {
                tracing::warn!(
                    "Redis backend requested but no pool provided, falling back to memory"
                );
                Arc::new(MemoryQueueBackend::new(config))
            }
        }
        _ => {
            tracing::info!(backend = "memory", "Creating memory queue backend");
            Arc::new(MemoryQueueBackend::new(config))
        }
    }
}

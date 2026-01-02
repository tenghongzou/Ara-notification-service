//! Factory function for creating session stores

use std::sync::Arc;

use crate::redis::pool::RedisPool;

use super::local::LocalSessionStore;
use super::redis_store::RedisSessionStore;
use super::traits::SessionStore;
use super::types::ClusterConfig;

/// Create a session store based on configuration
pub fn create_session_store(
    config: &ClusterConfig,
    redis_pool: Option<Arc<RedisPool>>,
) -> Arc<dyn SessionStore> {
    if config.enabled {
        if let Some(pool) = redis_pool {
            tracing::info!(
                server_id = %config.server_id,
                session_ttl = config.session_ttl_seconds,
                "Creating Redis session store for cluster mode"
            );
            Arc::new(RedisSessionStore::new(pool, config.clone()))
        } else {
            tracing::warn!(
                "Cluster mode enabled but Redis pool not available, falling back to local mode"
            );
            Arc::new(LocalSessionStore::new(config.server_id.clone()))
        }
    } else {
        tracing::info!("Cluster mode disabled, using local session store");
        Arc::new(LocalSessionStore::new(config.server_id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::types::SessionStoreBackend;

    #[test]
    fn test_create_local_session_store() {
        let config = ClusterConfig {
            enabled: false,
            ..Default::default()
        };

        let store = create_session_store(&config, None);
        assert!(!store.is_enabled());
        assert_eq!(store.backend_type(), SessionStoreBackend::Local);
    }
}

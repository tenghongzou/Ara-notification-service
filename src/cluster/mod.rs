//! Cluster module for distributed deployment support
//!
//! This module provides the infrastructure for running multiple notification
//! service instances that can coordinate through Redis.

mod factory;
mod local;
mod redis_store;
mod router;
mod traits;
mod types;

pub use factory::create_session_store;
pub use local::LocalSessionStore;
pub use redis_store::RedisSessionStore;
pub use router::{ClusterRouter, RouteResult, RoutedMessageSubscriber};
pub use traits::SessionStore;
pub use types::{ClusterConfig, RoutedMessage, SessionInfo, SessionStoreBackend, SessionStoreError};

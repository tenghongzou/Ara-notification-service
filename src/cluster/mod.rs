//! Cluster module for distributed deployment support
//!
//! This module provides the infrastructure for running multiple notification
//! service instances that can coordinate through Redis.

mod router;
mod session_store;

pub use router::{ClusterRouter, RouteResult, RoutedMessageSubscriber};
pub use session_store::{
    ClusterConfig, RoutedMessage, SessionInfo, SessionStore, SessionStoreBackend,
    SessionStoreError, create_session_store,
};

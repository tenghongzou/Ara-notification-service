//! Connection management for WebSocket connections
//!
//! This module provides:
//! - Connection handle management
//! - User and channel indexing
//! - Tenant isolation
//! - Connection statistics

mod manager;
mod stats;
mod types;

pub use manager::ConnectionManager;
pub use stats::{ChannelInfo, ConnectionStats, TenantConnectionStats, UserSubscriptionInfo};
pub use types::{ConnectionError, ConnectionHandle, ConnectionLimits};

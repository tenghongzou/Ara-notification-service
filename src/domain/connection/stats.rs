//! Connection statistics and info structures

use serde::Serialize;
use std::collections::HashMap;

/// Connection statistics
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStats {
    pub total_connections: usize,
    pub unique_users: usize,
    pub channels: HashMap<String, usize>,
}

/// Tenant-specific connection statistics
#[derive(Debug, Clone, Serialize)]
pub struct TenantConnectionStats {
    pub tenant_id: String,
    pub total_connections: usize,
    pub unique_users: usize,
}

/// Channel information
#[derive(Debug, Clone, Serialize)]
pub struct ChannelInfo {
    pub name: String,
    pub subscriber_count: usize,
}

/// User subscription information
#[derive(Debug, Clone, Serialize)]
pub struct UserSubscriptionInfo {
    pub user_id: String,
    pub connection_count: usize,
    pub subscriptions: Vec<String>,
}

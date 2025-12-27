use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::websocket::{OutboundMessage, ServerMessage};

/// Handle for a single WebSocket connection
pub struct ConnectionHandle {
    pub id: Uuid,
    pub user_id: String,
    pub tenant_id: String,
    pub roles: Vec<String>,
    pub sender: mpsc::Sender<OutboundMessage>,
    pub connected_at: DateTime<Utc>,
    /// Last activity timestamp (Unix seconds) - using AtomicI64 for lock-free updates
    last_activity: AtomicI64,
    pub subscriptions: RwLock<HashSet<String>>,
}

impl ConnectionHandle {
    pub fn new(
        user_id: String,
        tenant_id: String,
        roles: Vec<String>,
        sender: mpsc::Sender<OutboundMessage>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            tenant_id,
            roles,
            sender,
            connected_at: now,
            last_activity: AtomicI64::new(now.timestamp()),
            subscriptions: RwLock::new(HashSet::new()),
        }
    }

    pub fn update_activity(&self) {
        self.last_activity.store(Utc::now().timestamp(), Ordering::Relaxed);
    }

    pub fn last_activity(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.last_activity.load(Ordering::Relaxed), 0)
            .unwrap_or_else(Utc::now)
    }

    /// Send a ServerMessage (will be serialized when sent to WebSocket)
    pub async fn send(&self, message: ServerMessage) -> Result<(), mpsc::error::SendError<OutboundMessage>> {
        self.sender.send(OutboundMessage::Raw(message)).await
    }

    /// Send a pre-serialized message (for efficient multi-send scenarios)
    pub async fn send_preserialized(&self, message: OutboundMessage) -> Result<(), mpsc::error::SendError<OutboundMessage>> {
        self.sender.send(message).await
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Get current subscription count
    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }
}

/// Error returned when connection limits are exceeded
#[derive(Debug, Clone)]
pub enum ConnectionError {
    TotalLimitExceeded { current: usize, max: usize },
    UserLimitExceeded { user_id: String, current: usize, max: usize },
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TotalLimitExceeded { current, max } => {
                write!(f, "Total connection limit exceeded ({}/{})", current, max)
            }
            Self::UserLimitExceeded { user_id, current, max } => {
                write!(f, "User {} connection limit exceeded ({}/{})", user_id, current, max)
            }
        }
    }
}

/// Limits for connection management
#[derive(Debug, Clone, Copy)]
pub struct ConnectionLimits {
    pub max_connections: usize,
    pub max_connections_per_user: usize,
    pub max_subscriptions_per_connection: usize,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_connections: 10000,
            max_connections_per_user: 5,
            max_subscriptions_per_connection: 50,
        }
    }
}

/// Manages all active WebSocket connections
pub struct ConnectionManager {
    /// connection_id -> ConnectionHandle
    connections: DashMap<Uuid, Arc<ConnectionHandle>>,
    /// user_id -> Set<connection_id> (supports multiple devices)
    user_index: DashMap<String, HashSet<Uuid>>,
    /// channel_name -> Set<connection_id>
    channel_index: DashMap<String, HashSet<Uuid>>,
    /// tenant_id -> Set<connection_id> (for multi-tenant support)
    tenant_index: DashMap<String, HashSet<Uuid>>,
    /// Connection limits
    limits: ConnectionLimits,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self::with_limits(ConnectionLimits::default())
    }

    pub fn with_limits(limits: ConnectionLimits) -> Self {
        Self {
            connections: DashMap::new(),
            user_index: DashMap::new(),
            channel_index: DashMap::new(),
            tenant_index: DashMap::new(),
            limits,
        }
    }

    /// Register a new connection with limit checking
    pub fn register(
        &self,
        user_id: String,
        tenant_id: String,
        roles: Vec<String>,
        sender: mpsc::Sender<OutboundMessage>,
    ) -> Result<Arc<ConnectionHandle>, ConnectionError> {
        self.register_with_limits(user_id, tenant_id, roles, sender, &self.limits)
    }

    /// Register a new connection with custom limits (for per-tenant limits)
    pub fn register_with_limits(
        &self,
        user_id: String,
        tenant_id: String,
        roles: Vec<String>,
        sender: mpsc::Sender<OutboundMessage>,
        limits: &ConnectionLimits,
    ) -> Result<Arc<ConnectionHandle>, ConnectionError> {
        // Check total connection limit
        if limits.max_connections > 0 && self.connections.len() >= limits.max_connections {
            tracing::warn!(
                user_id = %user_id,
                tenant_id = %tenant_id,
                current = self.connections.len(),
                max = limits.max_connections,
                "Total connection limit exceeded"
            );
            return Err(ConnectionError::TotalLimitExceeded {
                current: self.connections.len(),
                max: limits.max_connections,
            });
        }

        // Check per-user connection limit
        if limits.max_connections_per_user > 0 {
            let user_conn_count = self.user_index
                .get(&user_id)
                .map(|c| c.len())
                .unwrap_or(0);

            if user_conn_count >= limits.max_connections_per_user {
                tracing::warn!(
                    user_id = %user_id,
                    tenant_id = %tenant_id,
                    current = user_conn_count,
                    max = limits.max_connections_per_user,
                    "User connection limit exceeded"
                );
                return Err(ConnectionError::UserLimitExceeded {
                    user_id: user_id.clone(),
                    current: user_conn_count,
                    max: limits.max_connections_per_user,
                });
            }
        }

        let handle = Arc::new(ConnectionHandle::new(
            user_id.clone(),
            tenant_id.clone(),
            roles,
            sender,
        ));
        let conn_id = handle.id;

        // Add to connections map
        self.connections.insert(conn_id, handle.clone());

        // Update user index
        self.user_index
            .entry(user_id)
            .or_default()
            .insert(conn_id);

        // Update tenant index
        self.tenant_index
            .entry(tenant_id.clone())
            .or_default()
            .insert(conn_id);

        tracing::info!(
            connection_id = %conn_id,
            user_id = %handle.user_id,
            tenant_id = %tenant_id,
            "Connection registered"
        );

        Ok(handle)
    }

    /// Unregister a connection
    pub async fn unregister(&self, connection_id: Uuid) {
        if let Some((_, handle)) = self.connections.remove(&connection_id) {
            // Remove from user index
            if let Some(mut user_conns) = self.user_index.get_mut(&handle.user_id) {
                user_conns.remove(&connection_id);
                if user_conns.is_empty() {
                    drop(user_conns);
                    self.user_index.remove(&handle.user_id);
                }
            }

            // Remove from tenant index
            if let Some(mut tenant_conns) = self.tenant_index.get_mut(&handle.tenant_id) {
                tenant_conns.remove(&connection_id);
                if tenant_conns.is_empty() {
                    drop(tenant_conns);
                    self.tenant_index.remove(&handle.tenant_id);
                }
            }

            // Remove only from channels this connection was subscribed to (optimized)
            let subscribed_channels = handle.subscriptions.read().await.clone();
            for channel in subscribed_channels {
                if let Some(mut channel_conns) = self.channel_index.get_mut(&channel) {
                    channel_conns.remove(&connection_id);
                    if channel_conns.is_empty() {
                        drop(channel_conns);
                        self.channel_index.remove(&channel);
                    }
                }
            }

            tracing::info!(
                connection_id = %connection_id,
                user_id = %handle.user_id,
                tenant_id = %handle.tenant_id,
                "Connection unregistered"
            );
        }
    }

    /// Subscribe a connection to a channel with limit checking
    pub async fn subscribe_to_channel(&self, connection_id: Uuid, channel: &str) -> Result<(), String> {
        if let Some(handle) = self.connections.get(&connection_id) {
            // Check subscription limit
            if self.limits.max_subscriptions_per_connection > 0 {
                let current_count = handle.subscription_count().await;
                if current_count >= self.limits.max_subscriptions_per_connection {
                    return Err(format!(
                        "Subscription limit exceeded ({}/{})",
                        current_count, self.limits.max_subscriptions_per_connection
                    ));
                }
            }

            // Update connection's subscriptions
            handle.subscriptions.write().await.insert(channel.to_string());

            // Update channel index
            self.channel_index
                .entry(channel.to_string())
                .or_default()
                .insert(connection_id);

            tracing::debug!(connection_id = %connection_id, channel = %channel, "Subscribed to channel");
            Ok(())
        } else {
            Err("Connection not found".to_string())
        }
    }

    /// Unsubscribe a connection from a channel
    pub async fn unsubscribe_from_channel(&self, connection_id: Uuid, channel: &str) {
        if let Some(handle) = self.connections.get(&connection_id) {
            // Update connection's subscriptions
            handle.subscriptions.write().await.remove(channel);

            // Update channel index
            if let Some(mut channel_conns) = self.channel_index.get_mut(channel) {
                channel_conns.remove(&connection_id);
                if channel_conns.is_empty() {
                    drop(channel_conns);
                    self.channel_index.remove(channel);
                }
            }

            tracing::debug!(connection_id = %connection_id, channel = %channel, "Unsubscribed from channel");
        }
    }

    /// Get all connections for a user
    pub fn get_user_connections(&self, user_id: &str) -> Vec<Arc<ConnectionHandle>> {
        self.user_index
            .get(user_id)
            .map(|conn_ids| {
                conn_ids
                    .iter()
                    .filter_map(|id| self.connections.get(id).map(|h| h.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all connections subscribed to a channel
    pub fn get_channel_connections(&self, channel: &str) -> Vec<Arc<ConnectionHandle>> {
        self.channel_index
            .get(channel)
            .map(|conn_ids| {
                conn_ids
                    .iter()
                    .filter_map(|id| self.connections.get(id).map(|h| h.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all connections
    pub fn get_all_connections(&self) -> Vec<Arc<ConnectionHandle>> {
        self.connections.iter().map(|r| r.value().clone()).collect()
    }

    /// Get connection by ID
    pub fn get_connection(&self, connection_id: Uuid) -> Option<Arc<ConnectionHandle>> {
        self.connections.get(&connection_id).map(|h| h.clone())
    }

    /// Get statistics
    pub fn stats(&self) -> ConnectionStats {
        let mut channel_counts = std::collections::HashMap::new();
        for entry in self.channel_index.iter() {
            channel_counts.insert(entry.key().clone(), entry.value().len());
        }

        ConnectionStats {
            total_connections: self.connections.len(),
            unique_users: self.user_index.len(),
            channels: channel_counts,
        }
    }

    /// Find connections that have been inactive for longer than the timeout
    /// This is now lock-free thanks to AtomicI64 for last_activity
    pub fn find_stale_connections(&self, timeout_secs: u64) -> Vec<Uuid> {
        let now = Utc::now();
        let timeout = chrono::Duration::seconds(timeout_secs as i64);
        let mut stale = Vec::new();

        for entry in self.connections.iter() {
            let last_activity = entry.value().last_activity();
            if now.signed_duration_since(last_activity) > timeout {
                stale.push(*entry.key());
            }
        }

        stale
    }

    /// Remove stale connections and return the count of removed connections
    pub async fn cleanup_stale_connections(&self, timeout_secs: u64) -> usize {
        let stale = self.find_stale_connections(timeout_secs);
        let count = stale.len();

        for conn_id in stale {
            tracing::info!(connection_id = %conn_id, "Removing stale connection due to timeout");
            self.unregister(conn_id).await;
        }

        count
    }

    /// Get all connection IDs (for heartbeat sending)
    pub fn get_all_connection_ids(&self) -> Vec<Uuid> {
        self.connections.iter().map(|r| *r.key()).collect()
    }

    /// List all channels with their subscriber counts
    pub fn list_channels(&self) -> Vec<ChannelInfo> {
        self.channel_index
            .iter()
            .map(|entry| ChannelInfo {
                name: entry.key().clone(),
                subscriber_count: entry.value().len(),
            })
            .collect()
    }

    /// Get info for a specific channel
    pub fn get_channel_info(&self, channel: &str) -> Option<ChannelInfo> {
        self.channel_index.get(channel).map(|entry| ChannelInfo {
            name: channel.to_string(),
            subscriber_count: entry.len(),
        })
    }

    /// Check if a channel exists
    pub fn channel_exists(&self, channel: &str) -> bool {
        self.channel_index.contains_key(channel)
    }

    /// Get all subscriptions for a user (across all their connections)
    pub async fn get_user_subscriptions(&self, user_id: &str) -> Option<UserSubscriptionInfo> {
        let connections = self.get_user_connections(user_id);
        if connections.is_empty() {
            return None;
        }

        let mut all_subscriptions = std::collections::HashSet::new();
        for conn in &connections {
            let subs = conn.subscriptions.read().await;
            all_subscriptions.extend(subs.iter().cloned());
        }

        Some(UserSubscriptionInfo {
            user_id: user_id.to_string(),
            connection_count: connections.len(),
            subscriptions: all_subscriptions.into_iter().collect(),
        })
    }

    // ========================================================================
    // Tenant-specific methods
    // ========================================================================

    /// Get all connections for a specific tenant
    pub fn get_tenant_connections(&self, tenant_id: &str) -> Vec<Arc<ConnectionHandle>> {
        self.tenant_index
            .get(tenant_id)
            .map(|conn_ids| {
                conn_ids
                    .iter()
                    .filter_map(|id| self.connections.get(id).map(|h| h.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get statistics for a specific tenant
    pub fn tenant_stats(&self, tenant_id: &str) -> TenantConnectionStats {
        let tenant_connections = self.get_tenant_connections(tenant_id);
        let unique_users: std::collections::HashSet<_> = tenant_connections
            .iter()
            .map(|c| c.user_id.clone())
            .collect();

        TenantConnectionStats {
            tenant_id: tenant_id.to_string(),
            total_connections: tenant_connections.len(),
            unique_users: unique_users.len(),
        }
    }

    /// Get connection count for a specific tenant
    pub fn tenant_connection_count(&self, tenant_id: &str) -> usize {
        self.tenant_index
            .get(tenant_id)
            .map(|conns| conns.len())
            .unwrap_or(0)
    }

    /// List all active tenant IDs
    pub fn list_tenants(&self) -> Vec<String> {
        self.tenant_index.iter().map(|e| e.key().clone()).collect()
    }

    /// List channels for a specific tenant (channels with at least one subscriber from the tenant)
    pub fn list_tenant_channels(&self, tenant_id: &str) -> Vec<ChannelInfo> {
        let tenant_connections: std::collections::HashSet<_> = self
            .tenant_index
            .get(tenant_id)
            .map(|ids| ids.iter().copied().collect())
            .unwrap_or_default();

        if tenant_connections.is_empty() {
            return vec![];
        }

        self.channel_index
            .iter()
            .filter_map(|entry| {
                let subscriber_count = entry
                    .value()
                    .iter()
                    .filter(|id| tenant_connections.contains(id))
                    .count();
                if subscriber_count > 0 {
                    Some(ChannelInfo {
                        name: entry.key().clone(),
                        subscriber_count,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Tenant-specific connection statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct TenantConnectionStats {
    pub tenant_id: String,
    pub total_connections: usize,
    pub unique_users: usize,
}

/// Channel information
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelInfo {
    pub name: String,
    pub subscriber_count: usize,
}

/// User subscription information
#[derive(Debug, Clone, serde::Serialize)]
pub struct UserSubscriptionInfo {
    pub user_id: String,
    pub connection_count: usize,
    pub subscriptions: Vec<String>,
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConnectionStats {
    pub total_connections: usize,
    pub unique_users: usize,
    pub channels: std::collections::HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_TENANT: &str = "default";

    fn create_test_manager() -> ConnectionManager {
        ConnectionManager::with_limits(ConnectionLimits {
            max_connections: 100,
            max_connections_per_user: 5,
            max_subscriptions_per_connection: 10,
        })
    }

    #[tokio::test]
    async fn test_list_channels_empty() {
        let manager = create_test_manager();
        let channels = manager.list_channels();
        assert!(channels.is_empty());
    }

    #[tokio::test]
    async fn test_list_channels_with_subscribers() {
        let manager = create_test_manager();
        let (tx, _rx) = mpsc::channel(32);

        // Register a connection and subscribe to channels
        let handle = manager
            .register("user-1".to_string(), DEFAULT_TENANT.to_string(), vec![], tx)
            .unwrap();

        manager.subscribe_to_channel(handle.id, "orders").await.unwrap();
        manager.subscribe_to_channel(handle.id, "alerts").await.unwrap();

        let channels = manager.list_channels();
        assert_eq!(channels.len(), 2);

        let order_channel = channels.iter().find(|c| c.name == "orders");
        assert!(order_channel.is_some());
        assert_eq!(order_channel.unwrap().subscriber_count, 1);
    }

    #[tokio::test]
    async fn test_list_channels_multiple_subscribers() {
        let manager = create_test_manager();

        // Register two users and subscribe to same channel
        let (tx1, _rx1) = mpsc::channel(32);
        let (tx2, _rx2) = mpsc::channel(32);

        let handle1 = manager.register("user-1".to_string(), DEFAULT_TENANT.to_string(), vec![], tx1).unwrap();
        let handle2 = manager.register("user-2".to_string(), DEFAULT_TENANT.to_string(), vec![], tx2).unwrap();

        manager.subscribe_to_channel(handle1.id, "orders").await.unwrap();
        manager.subscribe_to_channel(handle2.id, "orders").await.unwrap();

        let channels = manager.list_channels();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].subscriber_count, 2);
    }

    #[tokio::test]
    async fn test_get_channel_info_exists() {
        let manager = create_test_manager();
        let (tx, _rx) = mpsc::channel(32);

        let handle = manager.register("user-1".to_string(), DEFAULT_TENANT.to_string(), vec![], tx).unwrap();
        manager.subscribe_to_channel(handle.id, "orders").await.unwrap();

        let info = manager.get_channel_info("orders");
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.name, "orders");
        assert_eq!(info.subscriber_count, 1);
    }

    #[tokio::test]
    async fn test_get_channel_info_not_exists() {
        let manager = create_test_manager();
        let info = manager.get_channel_info("nonexistent");
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_channel_exists() {
        let manager = create_test_manager();
        let (tx, _rx) = mpsc::channel(32);

        let handle = manager.register("user-1".to_string(), DEFAULT_TENANT.to_string(), vec![], tx).unwrap();
        manager.subscribe_to_channel(handle.id, "orders").await.unwrap();

        assert!(manager.channel_exists("orders"));
        assert!(!manager.channel_exists("nonexistent"));
    }

    #[tokio::test]
    async fn test_get_user_subscriptions_exists() {
        let manager = create_test_manager();
        let (tx, _rx) = mpsc::channel(32);

        let handle = manager.register("user-1".to_string(), DEFAULT_TENANT.to_string(), vec![], tx).unwrap();
        manager.subscribe_to_channel(handle.id, "orders").await.unwrap();
        manager.subscribe_to_channel(handle.id, "alerts").await.unwrap();

        let subs = manager.get_user_subscriptions("user-1").await;
        assert!(subs.is_some());
        let subs = subs.unwrap();
        assert_eq!(subs.user_id, "user-1");
        assert_eq!(subs.connection_count, 1);
        assert_eq!(subs.subscriptions.len(), 2);
        assert!(subs.subscriptions.contains(&"orders".to_string()));
        assert!(subs.subscriptions.contains(&"alerts".to_string()));
    }

    #[tokio::test]
    async fn test_get_user_subscriptions_not_connected() {
        let manager = create_test_manager();
        let subs = manager.get_user_subscriptions("nonexistent").await;
        assert!(subs.is_none());
    }

    #[tokio::test]
    async fn test_get_user_subscriptions_multiple_connections() {
        let manager = create_test_manager();

        // Same user with two connections
        let (tx1, _rx1) = mpsc::channel(32);
        let (tx2, _rx2) = mpsc::channel(32);

        let handle1 = manager.register("user-1".to_string(), DEFAULT_TENANT.to_string(), vec![], tx1).unwrap();
        let handle2 = manager.register("user-1".to_string(), DEFAULT_TENANT.to_string(), vec![], tx2).unwrap();

        // Different subscriptions on each connection
        manager.subscribe_to_channel(handle1.id, "orders").await.unwrap();
        manager.subscribe_to_channel(handle2.id, "alerts").await.unwrap();
        manager.subscribe_to_channel(handle2.id, "orders").await.unwrap(); // Duplicate

        let subs = manager.get_user_subscriptions("user-1").await;
        assert!(subs.is_some());
        let subs = subs.unwrap();
        assert_eq!(subs.user_id, "user-1");
        assert_eq!(subs.connection_count, 2);
        // Should be deduplicated
        assert_eq!(subs.subscriptions.len(), 2);
    }

    #[tokio::test]
    async fn test_channel_removed_when_last_subscriber_leaves() {
        let manager = create_test_manager();
        let (tx, _rx) = mpsc::channel(32);

        let handle = manager.register("user-1".to_string(), DEFAULT_TENANT.to_string(), vec![], tx).unwrap();
        manager.subscribe_to_channel(handle.id, "orders").await.unwrap();

        assert!(manager.channel_exists("orders"));

        // Unsubscribe
        manager.unsubscribe_from_channel(handle.id, "orders").await;

        // Channel should be removed
        assert!(!manager.channel_exists("orders"));
    }
}

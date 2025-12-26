use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::websocket::ServerMessage;

/// Handle for a single WebSocket connection
pub struct ConnectionHandle {
    pub id: Uuid,
    pub user_id: String,
    pub sender: mpsc::Sender<ServerMessage>,
    pub connected_at: DateTime<Utc>,
    pub last_activity: RwLock<DateTime<Utc>>,
    pub subscriptions: RwLock<HashSet<String>>,
}

impl ConnectionHandle {
    pub fn new(user_id: String, sender: mpsc::Sender<ServerMessage>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            sender,
            connected_at: now,
            last_activity: RwLock::new(now),
            subscriptions: RwLock::new(HashSet::new()),
        }
    }

    pub async fn update_activity(&self) {
        let mut last = self.last_activity.write().await;
        *last = Utc::now();
    }

    pub async fn send(&self, message: ServerMessage) -> Result<(), mpsc::error::SendError<ServerMessage>> {
        self.sender.send(message).await
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
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            user_index: DashMap::new(),
            channel_index: DashMap::new(),
        }
    }

    /// Register a new connection
    pub fn register(&self, user_id: String, sender: mpsc::Sender<ServerMessage>) -> Arc<ConnectionHandle> {
        let handle = Arc::new(ConnectionHandle::new(user_id.clone(), sender));
        let conn_id = handle.id;

        // Add to connections map
        self.connections.insert(conn_id, handle.clone());

        // Update user index
        self.user_index
            .entry(user_id)
            .or_default()
            .insert(conn_id);

        tracing::info!(connection_id = %conn_id, user_id = %handle.user_id, "Connection registered");

        handle
    }

    /// Unregister a connection
    pub fn unregister(&self, connection_id: Uuid) {
        if let Some((_, handle)) = self.connections.remove(&connection_id) {
            // Remove from user index
            if let Some(mut user_conns) = self.user_index.get_mut(&handle.user_id) {
                user_conns.remove(&connection_id);
                if user_conns.is_empty() {
                    drop(user_conns);
                    self.user_index.remove(&handle.user_id);
                }
            }

            // Remove from all channel subscriptions
            for mut entry in self.channel_index.iter_mut() {
                entry.value_mut().remove(&connection_id);
            }

            // Clean up empty channels
            self.channel_index.retain(|_, conns| !conns.is_empty());

            tracing::info!(connection_id = %connection_id, user_id = %handle.user_id, "Connection unregistered");
        }
    }

    /// Subscribe a connection to a channel
    pub async fn subscribe_to_channel(&self, connection_id: Uuid, channel: &str) {
        if let Some(handle) = self.connections.get(&connection_id) {
            // Update connection's subscriptions
            handle.subscriptions.write().await.insert(channel.to_string());

            // Update channel index
            self.channel_index
                .entry(channel.to_string())
                .or_default()
                .insert(connection_id);

            tracing::debug!(connection_id = %connection_id, channel = %channel, "Subscribed to channel");
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
    pub async fn find_stale_connections(&self, timeout_secs: u64) -> Vec<Uuid> {
        let now = Utc::now();
        let timeout = chrono::Duration::seconds(timeout_secs as i64);
        let mut stale = Vec::new();

        for entry in self.connections.iter() {
            let last_activity = *entry.value().last_activity.read().await;
            if now.signed_duration_since(last_activity) > timeout {
                stale.push(*entry.key());
            }
        }

        stale
    }

    /// Remove stale connections and return the count of removed connections
    pub async fn cleanup_stale_connections(&self, timeout_secs: u64) -> usize {
        let stale = self.find_stale_connections(timeout_secs).await;
        let count = stale.len();

        for conn_id in stale {
            tracing::info!(connection_id = %conn_id, "Removing stale connection due to timeout");
            self.unregister(conn_id);
        }

        count
    }

    /// Get all connection IDs (for heartbeat sending)
    pub fn get_all_connection_ids(&self) -> Vec<Uuid> {
        self.connections.iter().map(|r| *r.key()).collect()
    }
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

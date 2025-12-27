//! Session store for distributed connection tracking
//!
//! Provides a trait-based abstraction for tracking which connections exist on
//! which server instances, enabling message routing in a multi-instance deployment.

use async_trait::async_trait;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::redis::pool::RedisPool;

/// Configuration for cluster mode
#[derive(Debug, Clone, Deserialize)]
pub struct ClusterConfig {
    /// Whether cluster mode is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Unique identifier for this server instance
    #[serde(default = "default_server_id")]
    pub server_id: String,
    /// Redis key prefix for session data
    #[serde(default = "default_session_prefix")]
    pub session_prefix: String,
    /// Session TTL in seconds (should be > heartbeat interval)
    #[serde(default = "default_session_ttl")]
    pub session_ttl_seconds: u64,
    /// Channel for routing messages between instances
    #[serde(default = "default_routing_channel")]
    pub routing_channel: String,
}

fn default_server_id() -> String {
    // Generate a unique ID for this instance
    format!("ara-{}", Uuid::new_v4().simple())
}

fn default_session_prefix() -> String {
    "ara:cluster:sessions".to_string()
}

fn default_session_ttl() -> u64 {
    60 // 1 minute (should be refreshed by heartbeat)
}

fn default_routing_channel() -> String {
    "ara:cluster:route".to_string()
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_id: default_server_id(),
            session_prefix: default_session_prefix(),
            session_ttl_seconds: default_session_ttl(),
            routing_channel: default_routing_channel(),
        }
    }
}

/// Information about a connection session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub connection_id: Uuid,
    pub user_id: String,
    pub tenant_id: String,
    pub server_id: String,
    pub connected_at: i64,
    pub channels: Vec<String>,
}

/// Message to be routed to another server instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedMessage {
    /// Target user ID
    pub user_id: String,
    /// Target tenant ID
    pub tenant_id: String,
    /// Optional: specific connection ID (if targeting a specific connection)
    pub connection_id: Option<Uuid>,
    /// The serialized message payload (JSON string of ServerMessage)
    pub payload: String,
    /// Source server ID
    pub from_server: String,
    /// Target server ID (if known)
    pub to_server: Option<String>,
}

/// Error type for session store operations
#[derive(Debug, Clone)]
pub enum SessionStoreError {
    /// Redis operation failed
    RedisError(String),
    /// Serialization/deserialization failed
    SerializationError(String),
    /// Store is disabled
    Disabled,
}

impl std::fmt::Display for SessionStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RedisError(msg) => write!(f, "Redis error: {}", msg),
            Self::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            Self::Disabled => write!(f, "Session store is disabled"),
        }
    }
}

impl std::error::Error for SessionStoreError {}

/// Backend type for session store
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStoreBackend {
    /// Local-only (no distributed tracking)
    Local,
    /// Redis-backed distributed session store
    Redis,
}

/// Trait for distributed session tracking
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Get the server ID for this instance
    fn server_id(&self) -> &str;

    /// Whether the store is enabled for distributed tracking
    fn is_enabled(&self) -> bool;

    /// Get the backend type
    fn backend_type(&self) -> SessionStoreBackend;

    /// Register a new connection session
    async fn register_session(&self, session: &SessionInfo) -> Result<(), SessionStoreError>;

    /// Unregister a connection session
    async fn unregister_session(&self, connection_id: Uuid) -> Result<(), SessionStoreError>;

    /// Update session channels (subscriptions)
    async fn update_session_channels(
        &self,
        connection_id: Uuid,
        channels: Vec<String>,
    ) -> Result<(), SessionStoreError>;

    /// Refresh session TTL (called during heartbeat)
    async fn refresh_sessions(&self) -> Result<usize, SessionStoreError>;

    /// Find which servers have a specific user connected
    async fn find_user_servers(&self, user_id: &str) -> Result<Vec<String>, SessionStoreError>;

    /// Find which servers have subscribers to a channel
    async fn find_channel_servers(&self, channel: &str) -> Result<Vec<String>, SessionStoreError>;

    /// Publish a routed message for another server to handle
    async fn publish_routed_message(&self, message: &RoutedMessage) -> Result<(), SessionStoreError>;

    /// Get cluster-wide connection count
    async fn cluster_connection_count(&self) -> Result<usize, SessionStoreError>;

    /// Get cluster-wide user count
    async fn cluster_user_count(&self) -> Result<usize, SessionStoreError>;

    /// Get all sessions across the cluster
    async fn get_all_sessions(&self) -> Result<Vec<SessionInfo>, SessionStoreError>;

    /// Get all sessions for a specific user
    async fn get_user_sessions(&self, user_id: &str) -> Result<Vec<SessionInfo>, SessionStoreError>;
}

/// Local-only session store (no distributed tracking)
pub struct LocalSessionStore {
    server_id: String,
}

impl LocalSessionStore {
    pub fn new(server_id: String) -> Self {
        Self { server_id }
    }
}

#[async_trait]
impl SessionStore for LocalSessionStore {
    fn server_id(&self) -> &str {
        &self.server_id
    }

    fn is_enabled(&self) -> bool {
        false
    }

    fn backend_type(&self) -> SessionStoreBackend {
        SessionStoreBackend::Local
    }

    async fn register_session(&self, _session: &SessionInfo) -> Result<(), SessionStoreError> {
        // No-op for local store
        Ok(())
    }

    async fn unregister_session(&self, _connection_id: Uuid) -> Result<(), SessionStoreError> {
        // No-op for local store
        Ok(())
    }

    async fn update_session_channels(
        &self,
        _connection_id: Uuid,
        _channels: Vec<String>,
    ) -> Result<(), SessionStoreError> {
        // No-op for local store
        Ok(())
    }

    async fn refresh_sessions(&self) -> Result<usize, SessionStoreError> {
        // No-op for local store
        Ok(0)
    }

    async fn find_user_servers(&self, _user_id: &str) -> Result<Vec<String>, SessionStoreError> {
        // Only this server in local mode
        Ok(vec![self.server_id.clone()])
    }

    async fn find_channel_servers(&self, _channel: &str) -> Result<Vec<String>, SessionStoreError> {
        // Only this server in local mode
        Ok(vec![self.server_id.clone()])
    }

    async fn publish_routed_message(&self, _message: &RoutedMessage) -> Result<(), SessionStoreError> {
        // No routing in local mode
        Err(SessionStoreError::Disabled)
    }

    async fn cluster_connection_count(&self) -> Result<usize, SessionStoreError> {
        // Not applicable in local mode
        Err(SessionStoreError::Disabled)
    }

    async fn cluster_user_count(&self) -> Result<usize, SessionStoreError> {
        // Not applicable in local mode
        Err(SessionStoreError::Disabled)
    }

    async fn get_all_sessions(&self) -> Result<Vec<SessionInfo>, SessionStoreError> {
        // Local mode returns empty - no distributed tracking
        Ok(vec![])
    }

    async fn get_user_sessions(&self, _user_id: &str) -> Result<Vec<SessionInfo>, SessionStoreError> {
        // Local mode returns empty - no distributed tracking
        Ok(vec![])
    }
}

/// Redis-backed distributed session store
pub struct RedisSessionStore {
    server_id: String,
    pool: Arc<RedisPool>,
    config: ClusterConfig,
    /// Local cache of connection IDs for this server (for refresh)
    local_connections: dashmap::DashSet<Uuid>,
}

impl RedisSessionStore {
    pub fn new(pool: Arc<RedisPool>, config: ClusterConfig) -> Self {
        Self {
            server_id: config.server_id.clone(),
            pool,
            config,
            local_connections: dashmap::DashSet::new(),
        }
    }

    /// Generate Redis key for a connection session
    fn session_key(&self, connection_id: Uuid) -> String {
        format!("{}:conn:{}", self.config.session_prefix, connection_id)
    }

    /// Generate Redis key for user -> servers mapping
    fn user_servers_key(&self, user_id: &str) -> String {
        format!("{}:user:{}", self.config.session_prefix, user_id)
    }

    /// Generate Redis key for channel -> servers mapping
    fn channel_servers_key(&self, channel: &str) -> String {
        format!("{}:channel:{}", self.config.session_prefix, channel)
    }

    /// Generate Redis key for server connection count
    fn server_connections_key(&self, server_id: &str) -> String {
        format!("{}:server:{}", self.config.session_prefix, server_id)
    }

    /// Generate Redis key for all users set
    fn all_users_key(&self) -> String {
        format!("{}:users", self.config.session_prefix)
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    fn server_id(&self) -> &str {
        &self.server_id
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn backend_type(&self) -> SessionStoreBackend {
        SessionStoreBackend::Redis
    }

    async fn register_session(&self, session: &SessionInfo) -> Result<(), SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        let session_json = serde_json::to_string(session)
            .map_err(|e| SessionStoreError::SerializationError(e.to_string()))?;

        let ttl = self.config.session_ttl_seconds as i64;

        // Use a pipeline for atomic operations
        let _: () = redis::pipe()
            // Store session data
            .cmd("SET")
            .arg(&self.session_key(session.connection_id))
            .arg(&session_json)
            .arg("EX")
            .arg(ttl)
            // Add server to user's server set
            .cmd("SADD")
            .arg(&self.user_servers_key(&session.user_id))
            .arg(&self.server_id)
            // Set TTL on user's server set
            .cmd("EXPIRE")
            .arg(&self.user_servers_key(&session.user_id))
            .arg(ttl)
            // Add user to global users set
            .cmd("SADD")
            .arg(&self.all_users_key())
            .arg(&session.user_id)
            // Increment server connection count
            .cmd("INCR")
            .arg(&self.server_connections_key(&self.server_id))
            // Set TTL on server count
            .cmd("EXPIRE")
            .arg(&self.server_connections_key(&self.server_id))
            .arg(ttl)
            .query_async(&mut conn)
            .await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        // Track locally for refresh
        self.local_connections.insert(session.connection_id);

        tracing::debug!(
            connection_id = %session.connection_id,
            user_id = %session.user_id,
            server_id = %self.server_id,
            "Session registered in cluster"
        );

        Ok(())
    }

    async fn unregister_session(&self, connection_id: Uuid) -> Result<(), SessionStoreError> {
        // Remove from local tracking
        self.local_connections.remove(&connection_id);

        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        // First, get the session to know the user_id
        let session_key = self.session_key(connection_id);
        let session_json: Option<String> = conn.get(&session_key).await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        if let Some(json) = session_json {
            if let Ok(session) = serde_json::from_str::<SessionInfo>(&json) {
                // Remove session and update indices
                let _: () = redis::pipe()
                    // Delete session data
                    .cmd("DEL")
                    .arg(&session_key)
                    // Remove server from user's server set (if no other connections from this server)
                    .cmd("SREM")
                    .arg(&self.user_servers_key(&session.user_id))
                    .arg(&self.server_id)
                    // Decrement server connection count
                    .cmd("DECR")
                    .arg(&self.server_connections_key(&self.server_id))
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

                // Remove from channel indices
                for channel in &session.channels {
                    let _: () = conn.srem(&self.channel_servers_key(channel), &self.server_id).await
                        .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;
                }

                tracing::debug!(
                    connection_id = %connection_id,
                    user_id = %session.user_id,
                    server_id = %self.server_id,
                    "Session unregistered from cluster"
                );
            }
        }

        Ok(())
    }

    async fn update_session_channels(
        &self,
        connection_id: Uuid,
        channels: Vec<String>,
    ) -> Result<(), SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        let session_key = self.session_key(connection_id);
        let ttl = self.config.session_ttl_seconds as i64;

        // Get current session
        let session_json: Option<String> = conn.get(&session_key).await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        if let Some(json) = session_json {
            if let Ok(mut session) = serde_json::from_str::<SessionInfo>(&json) {
                let old_channels = std::mem::replace(&mut session.channels, channels.clone());

                // Update session data
                let updated_json = serde_json::to_string(&session)
                    .map_err(|e| SessionStoreError::SerializationError(e.to_string()))?;

                let _: () = conn.set_ex(&session_key, &updated_json, ttl as u64).await
                    .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

                // Update channel indices
                // Remove from old channels not in new set
                for channel in &old_channels {
                    if !channels.contains(channel) {
                        let _: () = conn.srem(&self.channel_servers_key(channel), &self.server_id).await
                            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;
                    }
                }

                // Add to new channels
                for channel in &channels {
                    let key = self.channel_servers_key(channel);
                    let _: () = redis::pipe()
                        .cmd("SADD")
                        .arg(&key)
                        .arg(&self.server_id)
                        .cmd("EXPIRE")
                        .arg(&key)
                        .arg(ttl)
                        .query_async(&mut conn)
                        .await
                        .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;
                }
            }
        }

        Ok(())
    }

    async fn refresh_sessions(&self) -> Result<usize, SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        let ttl = self.config.session_ttl_seconds as i64;
        let mut refreshed = 0;

        // Refresh all local connections
        for connection_id in self.local_connections.iter() {
            let session_key = self.session_key(*connection_id);

            // Refresh TTL
            let result: i32 = conn.expire(&session_key, ttl).await
                .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

            if result == 1 {
                refreshed += 1;
            } else {
                // Session expired or doesn't exist, remove from local tracking
                self.local_connections.remove(&*connection_id);
            }
        }

        // Also refresh server connection count
        let _: () = conn.expire(&self.server_connections_key(&self.server_id), ttl).await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        if refreshed > 0 {
            tracing::debug!(
                server_id = %self.server_id,
                refreshed = refreshed,
                "Refreshed cluster sessions"
            );
        }

        Ok(refreshed)
    }

    async fn find_user_servers(&self, user_id: &str) -> Result<Vec<String>, SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        let servers: Vec<String> = conn.smembers(&self.user_servers_key(user_id)).await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        Ok(servers)
    }

    async fn find_channel_servers(&self, channel: &str) -> Result<Vec<String>, SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        let servers: Vec<String> = conn.smembers(&self.channel_servers_key(channel)).await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        Ok(servers)
    }

    async fn publish_routed_message(&self, message: &RoutedMessage) -> Result<(), SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        let message_json = serde_json::to_string(message)
            .map_err(|e| SessionStoreError::SerializationError(e.to_string()))?;

        // Determine routing channel
        let channel = if let Some(ref target_server) = message.to_server {
            format!("{}:{}", self.config.routing_channel, target_server)
        } else {
            // Broadcast to all servers
            self.config.routing_channel.clone()
        };

        let _: () = conn.publish(&channel, &message_json).await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        tracing::debug!(
            from_server = %message.from_server,
            to_server = ?message.to_server,
            user_id = %message.user_id,
            "Published routed message"
        );

        Ok(())
    }

    async fn cluster_connection_count(&self) -> Result<usize, SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        // Get all server keys and sum their connection counts
        let pattern = format!("{}:server:*", self.config.session_prefix);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        if keys.is_empty() {
            return Ok(0);
        }

        let mut total: usize = 0;
        for key in keys {
            let count: Option<i64> = conn.get(&key).await
                .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;
            if let Some(c) = count {
                if c > 0 {
                    total += c as usize;
                }
            }
        }

        Ok(total)
    }

    async fn cluster_user_count(&self) -> Result<usize, SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        let count: usize = conn.scard(&self.all_users_key()).await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        Ok(count)
    }

    async fn get_all_sessions(&self) -> Result<Vec<SessionInfo>, SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        // Get all session keys
        let pattern = format!("{}:conn:*", self.config.session_prefix);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        if keys.is_empty() {
            return Ok(vec![]);
        }

        // Get all session data
        let mut sessions = Vec::with_capacity(keys.len());
        for key in keys {
            let json: Option<String> = conn.get(&key).await
                .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;
            if let Some(data) = json {
                if let Ok(session) = serde_json::from_str::<SessionInfo>(&data) {
                    sessions.push(session);
                }
            }
        }

        Ok(sessions)
    }

    async fn get_user_sessions(&self, user_id: &str) -> Result<Vec<SessionInfo>, SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        // Get all session keys
        let pattern = format!("{}:conn:*", self.config.session_prefix);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        if keys.is_empty() {
            return Ok(vec![]);
        }

        // Filter sessions by user_id
        let mut sessions = Vec::new();
        for key in keys {
            let json: Option<String> = conn.get(&key).await
                .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;
            if let Some(data) = json {
                if let Ok(session) = serde_json::from_str::<SessionInfo>(&data) {
                    if session.user_id == user_id {
                        sessions.push(session);
                    }
                }
            }
        }

        Ok(sessions)
    }
}

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

    #[test]
    fn test_default_config() {
        let config = ClusterConfig::default();
        assert!(!config.enabled);
        assert!(config.server_id.starts_with("ara-"));
        assert_eq!(config.session_prefix, "ara:cluster:sessions");
        assert_eq!(config.session_ttl_seconds, 60);
    }

    #[test]
    fn test_session_info_serialization() {
        let session = SessionInfo {
            connection_id: Uuid::new_v4(),
            user_id: "user1".to_string(),
            tenant_id: "tenant1".to_string(),
            server_id: "server1".to_string(),
            connected_at: 1234567890,
            channels: vec!["orders".to_string(), "alerts".to_string()],
        };

        let json = serde_json::to_string(&session).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.connection_id, session.connection_id);
        assert_eq!(parsed.user_id, session.user_id);
        assert_eq!(parsed.channels.len(), 2);
    }

    #[test]
    fn test_routed_message_serialization() {
        let msg = RoutedMessage {
            user_id: "user1".to_string(),
            tenant_id: "tenant1".to_string(),
            connection_id: Some(Uuid::new_v4()),
            payload: r#"{"type":"notification"}"#.to_string(),
            from_server: "server1".to_string(),
            to_server: Some("server2".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: RoutedMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.user_id, msg.user_id);
        assert!(parsed.to_server.is_some());
    }

    #[tokio::test]
    async fn test_local_session_store() {
        let store = LocalSessionStore::new("test-server".to_string());

        assert_eq!(store.server_id(), "test-server");
        assert!(!store.is_enabled());
        assert_eq!(store.backend_type(), SessionStoreBackend::Local);

        // All operations should succeed (no-op)
        let session = SessionInfo {
            connection_id: Uuid::new_v4(),
            user_id: "user1".to_string(),
            tenant_id: "tenant1".to_string(),
            server_id: "test-server".to_string(),
            connected_at: 1234567890,
            channels: vec![],
        };

        assert!(store.register_session(&session).await.is_ok());
        assert!(store.unregister_session(session.connection_id).await.is_ok());

        // Find operations return local server only
        let servers = store.find_user_servers("user1").await.unwrap();
        assert_eq!(servers, vec!["test-server"]);

        // Routing should fail
        let msg = RoutedMessage {
            user_id: "user1".to_string(),
            tenant_id: "tenant1".to_string(),
            connection_id: None,
            payload: "{}".to_string(),
            from_server: "test-server".to_string(),
            to_server: None,
        };
        assert!(store.publish_routed_message(&msg).await.is_err());
    }

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

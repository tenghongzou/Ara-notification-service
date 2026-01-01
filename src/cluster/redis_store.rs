//! Redis-backed distributed session store

use async_trait::async_trait;
use redis::AsyncCommands;
use std::sync::Arc;
use uuid::Uuid;

use crate::redis::pool::RedisPool;

use super::traits::SessionStore;
use super::types::{
    ClusterConfig, RoutedMessage, SessionInfo, SessionStoreBackend, SessionStoreError,
};

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
        let session_json: Option<String> = conn
            .get(&session_key)
            .await
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
                    let _: () = conn
                        .srem(&self.channel_servers_key(channel), &self.server_id)
                        .await
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
        let session_json: Option<String> = conn
            .get(&session_key)
            .await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        if let Some(json) = session_json {
            if let Ok(mut session) = serde_json::from_str::<SessionInfo>(&json) {
                let old_channels = std::mem::replace(&mut session.channels, channels.clone());

                // Update session data
                let updated_json = serde_json::to_string(&session)
                    .map_err(|e| SessionStoreError::SerializationError(e.to_string()))?;

                let _: () = conn
                    .set_ex(&session_key, &updated_json, ttl as u64)
                    .await
                    .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

                // Update channel indices
                // Remove from old channels not in new set
                for channel in &old_channels {
                    if !channels.contains(channel) {
                        let _: () = conn
                            .srem(&self.channel_servers_key(channel), &self.server_id)
                            .await
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
            let result: i32 = conn
                .expire(&session_key, ttl)
                .await
                .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

            if result == 1 {
                refreshed += 1;
            } else {
                // Session expired or doesn't exist, remove from local tracking
                self.local_connections.remove(&*connection_id);
            }
        }

        // Also refresh server connection count
        let _: () = conn
            .expire(&self.server_connections_key(&self.server_id), ttl)
            .await
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

        let servers: Vec<String> = conn
            .smembers(&self.user_servers_key(user_id))
            .await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        Ok(servers)
    }

    async fn find_channel_servers(&self, channel: &str) -> Result<Vec<String>, SessionStoreError> {
        let mut conn = self.pool.get_connection().await.map_err(|e| {
            SessionStoreError::RedisError(format!("Failed to get connection: {}", e))
        })?;

        let servers: Vec<String> = conn
            .smembers(&self.channel_servers_key(channel))
            .await
            .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;

        Ok(servers)
    }

    async fn publish_routed_message(
        &self,
        message: &RoutedMessage,
    ) -> Result<(), SessionStoreError> {
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

        let _: () = conn
            .publish(&channel, &message_json)
            .await
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
            let count: Option<i64> = conn
                .get(&key)
                .await
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

        let count: usize = conn
            .scard(&self.all_users_key())
            .await
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
            let json: Option<String> = conn
                .get(&key)
                .await
                .map_err(|e| SessionStoreError::RedisError(e.to_string()))?;
            if let Some(data) = json {
                if let Ok(session) = serde_json::from_str::<SessionInfo>(&data) {
                    sessions.push(session);
                }
            }
        }

        Ok(sessions)
    }

    async fn get_user_sessions(
        &self,
        user_id: &str,
    ) -> Result<Vec<SessionInfo>, SessionStoreError> {
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
            let json: Option<String> = conn
                .get(&key)
                .await
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

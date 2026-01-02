//! Session store trait definition

use async_trait::async_trait;
use uuid::Uuid;

use super::types::{RoutedMessage, SessionInfo, SessionStoreBackend, SessionStoreError};

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
    async fn publish_routed_message(&self, message: &RoutedMessage)
        -> Result<(), SessionStoreError>;

    /// Get cluster-wide connection count
    async fn cluster_connection_count(&self) -> Result<usize, SessionStoreError>;

    /// Get cluster-wide user count
    async fn cluster_user_count(&self) -> Result<usize, SessionStoreError>;

    /// Get all sessions across the cluster
    async fn get_all_sessions(&self) -> Result<Vec<SessionInfo>, SessionStoreError>;

    /// Get all sessions for a specific user
    async fn get_user_sessions(
        &self,
        user_id: &str,
    ) -> Result<Vec<SessionInfo>, SessionStoreError>;
}

//! Local-only session store (no distributed tracking)

use async_trait::async_trait;
use uuid::Uuid;

use super::traits::SessionStore;
use super::types::{RoutedMessage, SessionInfo, SessionStoreBackend, SessionStoreError};

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

    async fn publish_routed_message(
        &self,
        _message: &RoutedMessage,
    ) -> Result<(), SessionStoreError> {
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

    async fn get_user_sessions(
        &self,
        _user_id: &str,
    ) -> Result<Vec<SessionInfo>, SessionStoreError> {
        // Local mode returns empty - no distributed tracking
        Ok(vec![])
    }
}

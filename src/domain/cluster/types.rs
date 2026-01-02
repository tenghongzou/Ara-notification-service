//! Cluster-related types and configuration

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

//! Connection handle and related types

use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
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
        self.last_activity
            .store(Utc::now().timestamp(), Ordering::Relaxed);
    }

    pub fn last_activity(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.last_activity.load(Ordering::Relaxed), 0)
            .unwrap_or_else(Utc::now)
    }

    /// Send a ServerMessage (will be serialized when sent to WebSocket)
    pub async fn send(
        &self,
        message: ServerMessage,
    ) -> Result<(), mpsc::error::SendError<OutboundMessage>> {
        self.sender.send(OutboundMessage::Raw(message)).await
    }

    /// Send a pre-serialized message (for efficient multi-send scenarios)
    pub async fn send_preserialized(
        &self,
        message: OutboundMessage,
    ) -> Result<(), mpsc::error::SendError<OutboundMessage>> {
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
            Self::UserLimitExceeded {
                user_id,
                current,
                max,
            } => {
                write!(
                    f,
                    "User {} connection limit exceeded ({}/{})",
                    user_id, current, max
                )
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

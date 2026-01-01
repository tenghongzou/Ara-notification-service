use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::notification::NotificationEvent;

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessage {
    Subscribe { channels: Vec<String> },
    Unsubscribe { channels: Vec<String> },
    Ping,
    Ack { notification_id: Uuid },
}

/// Outbound message wrapper for efficient multi-send scenarios
/// When sending the same message to many connections, pre-serializing once
/// and sharing the Arc<str> avoids repeated serialization overhead
#[derive(Debug, Clone)]
pub enum OutboundMessage {
    /// Message that will be serialized when sent
    Raw(ServerMessage),
    /// Pre-serialized message (shared across multiple sends via Arc)
    Serialized(Arc<str>),
}

impl OutboundMessage {
    /// Create a pre-serialized message from a ServerMessage
    pub fn preserialized(message: &ServerMessage) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_string(message)?;
        Ok(Self::Serialized(Arc::from(json)))
    }

    /// Convert to JSON string, either by returning the pre-serialized string
    /// or by serializing the raw message
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        match self {
            Self::Raw(msg) => serde_json::to_string(msg),
            Self::Serialized(json) => Ok(json.to_string()),
        }
    }
}

impl From<ServerMessage> for OutboundMessage {
    fn from(msg: ServerMessage) -> Self {
        Self::Raw(msg)
    }
}

/// Messages sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "notification")]
    Notification {
        #[serde(flatten)]
        event: NotificationEvent,
    },
    #[serde(rename = "subscribed")]
    Subscribed {
        #[serde(rename = "payload")]
        channels: Vec<String>,
    },
    #[serde(rename = "unsubscribed")]
    Unsubscribed {
        #[serde(rename = "payload")]
        channels: Vec<String>,
    },
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "heartbeat")]
    Heartbeat,
    #[serde(rename = "acked")]
    Acked {
        notification_id: Uuid,
    },
    #[serde(rename = "error")]
    Error {
        code: String,
        message: String,
    },
    /// Server shutdown notification - sent to all clients before shutdown
    #[serde(rename = "shutdown")]
    Shutdown {
        /// Reason for shutdown
        reason: String,
        /// Seconds until server stops accepting new connections (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        reconnect_after_seconds: Option<u64>,
    },
}

impl ServerMessage {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn subscribed(channels: Vec<String>) -> Self {
        Self::Subscribed { channels }
    }

    pub fn unsubscribed(channels: Vec<String>) -> Self {
        Self::Unsubscribed { channels }
    }

    pub fn acked(notification_id: Uuid) -> Self {
        Self::Acked { notification_id }
    }

    pub fn shutdown(reason: impl Into<String>, reconnect_after_seconds: Option<u64>) -> Self {
        Self::Shutdown {
            reason: reason.into(),
            reconnect_after_seconds,
        }
    }
}

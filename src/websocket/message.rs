use serde::{Deserialize, Serialize};

use crate::notification::NotificationEvent;

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessage {
    Subscribe { channels: Vec<String> },
    Unsubscribe { channels: Vec<String> },
    Ping,
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
    #[serde(rename = "error")]
    Error {
        code: String,
        message: String,
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
}

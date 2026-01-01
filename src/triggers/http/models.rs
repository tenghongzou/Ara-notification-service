//! Request and response models for HTTP notification API

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::notification::{Audience, Priority};

use super::content::NotificationContent;

/// Request to send notification to a specific user
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct SendNotificationRequest {
    /// Target user ID
    pub target_user_id: String,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Request to send notification to multiple users
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct SendToUsersRequest {
    /// Target user IDs
    pub target_user_ids: Vec<String>,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to broadcast notification to all users
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct BroadcastNotificationRequest {
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional target audience filter
    pub audience: Option<Audience>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to send notification to a channel
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct ChannelNotificationRequest {
    /// Target channel name
    pub channel: String,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to send notification to multiple channels
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct MultiChannelNotificationRequest {
    /// Target channel names
    pub channels: Vec<String>,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Response for notification send operations
#[derive(Debug, Serialize)]
pub struct SendNotificationResponse {
    /// Whether the operation was successful
    pub success: bool,
    /// Notification ID
    pub notification_id: Uuid,
    /// Number of connections the notification was delivered to
    pub delivered_to: usize,
    /// Number of failed deliveries
    pub failed: usize,
    /// Timestamp of the operation
    pub timestamp: DateTime<Utc>,
}

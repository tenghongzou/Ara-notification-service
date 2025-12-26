use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::notification::{Audience, NotificationBuilder, Priority};
use crate::server::AppState;

/// Request to send notification to a specific user
#[derive(Debug, Deserialize)]
pub struct SendNotificationRequest {
    /// Target user ID
    pub target_user_id: String,
    /// Event type (e.g., "order.created")
    pub event_type: String,
    /// Event payload
    pub payload: serde_json::Value,
    /// Priority level
    #[serde(default)]
    pub priority: Priority,
    /// Optional TTL in seconds
    pub ttl: Option<u32>,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Request to send notification to multiple users
#[derive(Debug, Deserialize)]
pub struct SendToUsersRequest {
    /// Target user IDs
    pub target_user_ids: Vec<String>,
    /// Event type
    pub event_type: String,
    /// Event payload
    pub payload: serde_json::Value,
    /// Priority level
    #[serde(default)]
    pub priority: Priority,
    /// Optional TTL in seconds
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to broadcast notification to all users
#[derive(Debug, Deserialize)]
pub struct BroadcastNotificationRequest {
    /// Event type
    pub event_type: String,
    /// Event payload
    pub payload: serde_json::Value,
    /// Priority level
    #[serde(default)]
    pub priority: Priority,
    /// Optional TTL in seconds
    pub ttl: Option<u32>,
    /// Optional target audience filter
    pub audience: Option<Audience>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to send notification to a channel
#[derive(Debug, Deserialize)]
pub struct ChannelNotificationRequest {
    /// Target channel name
    pub channel: String,
    /// Event type
    pub event_type: String,
    /// Event payload
    pub payload: serde_json::Value,
    /// Priority level
    #[serde(default)]
    pub priority: Priority,
    /// Optional TTL in seconds
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to send notification to multiple channels
#[derive(Debug, Deserialize)]
pub struct MultiChannelNotificationRequest {
    /// Target channel names
    pub channels: Vec<String>,
    /// Event type
    pub event_type: String,
    /// Event payload
    pub payload: serde_json::Value,
    /// Priority level
    #[serde(default)]
    pub priority: Priority,
    /// Optional TTL in seconds
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

const SOURCE: &str = "http-api";

/// Send notification to a specific user
pub async fn send_notification(
    State(state): State<AppState>,
    Json(request): Json<SendNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let mut builder = NotificationBuilder::new(&request.event_type, SOURCE)
        .payload(request.payload)
        .priority(request.priority);

    if let Some(ttl) = request.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state
        .dispatcher
        .send_to_user(&request.target_user_id, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

/// Send notification to multiple users
pub async fn send_to_users(
    State(state): State<AppState>,
    Json(request): Json<SendToUsersRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let mut builder = NotificationBuilder::new(&request.event_type, SOURCE)
        .payload(request.payload)
        .priority(request.priority);

    if let Some(ttl) = request.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state
        .dispatcher
        .send_to_users(&request.target_user_ids, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

/// Broadcast notification to all connected users
pub async fn broadcast_notification(
    State(state): State<AppState>,
    Json(request): Json<BroadcastNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let mut builder = NotificationBuilder::new(&request.event_type, SOURCE)
        .payload(request.payload)
        .priority(request.priority);

    if let Some(ttl) = request.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(audience) = request.audience {
        builder = builder.audience(audience);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state.dispatcher.broadcast(event).await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

/// Send notification to a channel
pub async fn channel_notification(
    State(state): State<AppState>,
    Json(request): Json<ChannelNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let mut builder = NotificationBuilder::new(&request.event_type, SOURCE)
        .payload(request.payload)
        .priority(request.priority);

    if let Some(ttl) = request.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state
        .dispatcher
        .send_to_channel(&request.channel, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

/// Send notification to multiple channels
pub async fn multi_channel_notification(
    State(state): State<AppState>,
    Json(request): Json<MultiChannelNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let mut builder = NotificationBuilder::new(&request.event_type, SOURCE)
        .payload(request.payload)
        .priority(request.priority);

    if let Some(ttl) = request.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state
        .dispatcher
        .send_to_channels(&request.channels, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

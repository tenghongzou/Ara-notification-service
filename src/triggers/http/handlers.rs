//! HTTP notification handlers

use axum::{extract::State, Json};
use chrono::Utc;

use crate::error::Result;
use crate::notification::NotificationBuilder;
use crate::server::AppState;

use super::models::{
    BroadcastNotificationRequest, ChannelNotificationRequest, MultiChannelNotificationRequest,
    SendNotificationRequest, SendNotificationResponse, SendToUsersRequest,
};

const SOURCE: &str = "http-api";

/// Send notification to a specific user
#[tracing::instrument(
    name = "http.send_notification",
    skip(state, request),
    fields(target_user_id = %request.target_user_id)
)]
pub async fn send_notification(
    State(state): State<AppState>,
    Json(request): Json<SendNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
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
#[tracing::instrument(
    name = "http.send_to_users",
    skip(state, request),
    fields(user_count = request.target_user_ids.len())
)]
pub async fn send_to_users(
    State(state): State<AppState>,
    Json(request): Json<SendToUsersRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
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
#[tracing::instrument(
    name = "http.broadcast",
    skip(state, request),
    fields(audience = ?request.audience)
)]
pub async fn broadcast_notification(
    State(state): State<AppState>,
    Json(request): Json<BroadcastNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
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
#[tracing::instrument(
    name = "http.channel_notification",
    skip(state, request),
    fields(channel = %request.channel)
)]
pub async fn channel_notification(
    State(state): State<AppState>,
    Json(request): Json<ChannelNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
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
#[tracing::instrument(
    name = "http.multi_channel_notification",
    skip(state, request),
    fields(channel_count = request.channels.len())
)]
pub async fn multi_channel_notification(
    State(state): State<AppState>,
    Json(request): Json<MultiChannelNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
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

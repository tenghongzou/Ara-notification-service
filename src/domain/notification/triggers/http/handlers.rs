//! HTTP notification handlers

use axum::{extract::State, Extension, Json};
use chrono::Utc;

use crate::error::Result;
use crate::notification::NotificationBuilder;
use crate::server::middleware::RequestTenantContext;
use crate::server::AppState;

use super::models::{
    BroadcastNotificationRequest, ChannelNotificationRequest, MultiChannelNotificationRequest,
    SendNotificationRequest, SendNotificationResponse, SendToUsersRequest,
};

const SOURCE: &str = "http-api";

/// Maximum number of target user IDs in a single send-to-users request
const MAX_TARGET_USERS: usize = 10_000;

/// Maximum number of channels in a single multi-channel request
const MAX_CHANNELS: usize = 100;

/// Send notification to a specific user
#[tracing::instrument(
    name = "http.send_notification",
    skip(state, request, tenant_ctx),
    fields(target_user_id = %request.target_user_id)
)]
pub async fn send_notification(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Json(request): Json<SendNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let tenant_id = tenant_ctx.as_ref().map(|t| t.0.tenant_id());

    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve_for_tenant(&state.template_store, tenant_id, request.priority, request.ttl)?;

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
        .dispatch_for_tenant(
            crate::notification::NotificationTarget::User(request.target_user_id),
            event,
            tenant_id,
        )
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
    skip(state, request, tenant_ctx),
    fields(user_count = request.target_user_ids.len())
)]
pub async fn send_to_users(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Json(request): Json<SendToUsersRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let tenant_id = tenant_ctx.as_ref().map(|t| t.0.tenant_id());

    // Validate array size
    if request.target_user_ids.len() > MAX_TARGET_USERS {
        return Err(crate::error::AppError::Validation(format!(
            "target_user_ids exceeds maximum of {} (got {})",
            MAX_TARGET_USERS,
            request.target_user_ids.len()
        )));
    }

    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve_for_tenant(&state.template_store, tenant_id, request.priority, request.ttl)?;

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
        .dispatch_for_tenant(
            crate::notification::NotificationTarget::Users(request.target_user_ids),
            event,
            tenant_id,
        )
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
    skip(state, request, tenant_ctx),
    fields(audience = ?request.audience)
)]
pub async fn broadcast_notification(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Json(request): Json<BroadcastNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let tenant_id = tenant_ctx.as_ref().map(|t| t.0.tenant_id());

    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve_for_tenant(&state.template_store, tenant_id, request.priority, request.ttl)?;

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
    let result = state
        .dispatcher
        .dispatch_for_tenant(
            crate::notification::NotificationTarget::Broadcast,
            event,
            tenant_id,
        )
        .await;

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
    skip(state, request, tenant_ctx),
    fields(channel = %request.channel)
)]
pub async fn channel_notification(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Json(request): Json<ChannelNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    let tenant_id = tenant_ctx.as_ref().map(|t| t.0.tenant_id());

    // Namespace channel for tenant isolation
    let channel = tenant_ctx
        .as_ref()
        .map(|t| t.0.namespace_channel(&request.channel))
        .unwrap_or_else(|| request.channel.clone());

    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve_for_tenant(&state.template_store, tenant_id, request.priority, request.ttl)?;

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
        .send_to_channel(&channel, event)
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
    skip(state, request, tenant_ctx),
    fields(channel_count = request.channels.len())
)]
pub async fn multi_channel_notification(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Json(request): Json<MultiChannelNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Validate array size
    if request.channels.len() > MAX_CHANNELS {
        return Err(crate::error::AppError::Validation(format!(
            "channels exceeds maximum of {} (got {})",
            MAX_CHANNELS,
            request.channels.len()
        )));
    }

    let tenant_id = tenant_ctx.as_ref().map(|t| t.0.tenant_id());

    // Namespace channels for tenant isolation
    let channels: Vec<String> = match tenant_ctx.as_ref() {
        Some(t) => request
            .channels
            .iter()
            .map(|ch| t.0.namespace_channel(ch))
            .collect(),
        None => request.channels,
    };

    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve_for_tenant(&state.template_store, tenant_id, request.priority, request.ttl)?;

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
        .send_to_channels(&channels, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

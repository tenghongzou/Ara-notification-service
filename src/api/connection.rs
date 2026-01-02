//! Connection and channel management endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::connection_manager::ChannelInfo;
use crate::server::AppState;

// ============================================================================
// Channel Endpoints
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ChannelListResponse {
    pub channels: Vec<ChannelInfo>,
    pub total_channels: usize,
}

#[derive(Debug, Serialize)]
pub struct ChannelDetailResponse {
    pub name: String,
    pub subscriber_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ChannelErrorResponse {
    pub error: ChannelError,
}

#[derive(Debug, Serialize)]
pub struct ChannelError {
    pub code: String,
    pub message: String,
}

/// GET /api/v1/channels - List all channels with subscriber counts
pub async fn list_channels(State(state): State<AppState>) -> Json<ChannelListResponse> {
    let channels = state.connection_manager.list_channels();
    let total = channels.len();

    Json(ChannelListResponse {
        channels,
        total_channels: total,
    })
}

/// GET /api/v1/channels/:name - Get channel details
pub async fn get_channel(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ChannelDetailResponse>, (StatusCode, Json<ChannelErrorResponse>)> {
    match state.connection_manager.get_channel_info(&name) {
        Some(info) => Ok(Json(ChannelDetailResponse {
            name: info.name,
            subscriber_count: info.subscriber_count,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ChannelErrorResponse {
                error: ChannelError {
                    code: "CHANNEL_NOT_FOUND".to_string(),
                    message: format!("Channel '{}' not found or has no subscribers", name),
                },
            }),
        )),
    }
}

// ============================================================================
// User Subscription Endpoints
// ============================================================================

#[derive(Debug, Serialize)]
pub struct UserSubscriptionsResponse {
    pub user_id: String,
    pub connection_count: usize,
    pub subscriptions: Vec<String>,
}

/// GET /api/v1/users/:user_id/subscriptions - Get user's subscriptions
pub async fn get_user_subscriptions(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<UserSubscriptionsResponse>, (StatusCode, Json<ChannelErrorResponse>)> {
    match state.connection_manager.get_user_subscriptions(&user_id).await {
        Some(info) => Ok(Json(UserSubscriptionsResponse {
            user_id: info.user_id,
            connection_count: info.connection_count,
            subscriptions: info.subscriptions,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ChannelErrorResponse {
                error: ChannelError {
                    code: "USER_NOT_CONNECTED".to_string(),
                    message: format!("User '{}' has no active connections", user_id),
                },
            }),
        )),
    }
}

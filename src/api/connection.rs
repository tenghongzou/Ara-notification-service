//! Connection and channel management endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Serialize;

use crate::connection_manager::ChannelInfo;
use crate::server::middleware::RequestTenantContext;
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

/// GET /api/v1/channels - List channels with subscriber counts (tenant-filtered)
pub async fn list_channels(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
) -> Json<ChannelListResponse> {
    let all_channels = state.connection_manager.list_channels();

    // Filter channels by tenant namespace when multi-tenancy is enabled
    let channels: Vec<ChannelInfo> = match tenant_ctx.as_ref() {
        Some(t) if !t.0 .0.is_default => {
            let prefix = format!("{}:", t.0.tenant_id());
            all_channels
                .into_iter()
                .filter(|ch| ch.name.starts_with(&prefix))
                .collect()
        }
        _ => all_channels,
    };
    let total = channels.len();

    Json(ChannelListResponse {
        channels,
        total_channels: total,
    })
}

/// GET /api/v1/channels/:name - Get channel details (tenant-scoped)
pub async fn get_channel(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Path(name): Path<String>,
) -> Result<Json<ChannelDetailResponse>, (StatusCode, Json<ChannelErrorResponse>)> {
    // Namespace the channel name for tenant isolation
    let namespaced = match tenant_ctx.as_ref() {
        Some(t) => t.0.namespace_channel(&name),
        None => name.clone(),
    };
    match state.connection_manager.get_channel_info(&namespaced) {
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

/// GET /api/v1/users/:user_id/subscriptions - Get user's subscriptions (tenant-filtered)
pub async fn get_user_subscriptions(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Path(user_id): Path<String>,
) -> Result<Json<UserSubscriptionsResponse>, (StatusCode, Json<ChannelErrorResponse>)> {
    match state.connection_manager.get_user_subscriptions(&user_id).await {
        Some(info) => {
            // Filter connections and subscriptions by tenant when multi-tenancy is enabled
            let (connection_count, subscriptions) = match tenant_ctx.as_ref() {
                Some(t) if !t.0 .0.is_default => {
                    let tenant_id = t.0.tenant_id();
                    let connections = state.connection_manager.get_user_connections(&user_id);
                    let tenant_conn_count = connections
                        .iter()
                        .filter(|c| c.tenant_id == tenant_id)
                        .count();
                    let prefix = format!("{}:", tenant_id);
                    let tenant_subs: Vec<String> = info
                        .subscriptions
                        .into_iter()
                        .filter(|s| s.starts_with(&prefix))
                        .collect();
                    (tenant_conn_count, tenant_subs)
                }
                _ => (info.connection_count, info.subscriptions),
            };
            Ok(Json(UserSubscriptionsResponse {
                user_id: info.user_id,
                connection_count,
                subscriptions,
            }))
        }
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

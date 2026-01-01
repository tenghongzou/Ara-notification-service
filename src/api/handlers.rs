use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::connection_manager::ChannelInfo;
use crate::server::AppState;
use crate::tenant::{TenantInfo, TenantStatsSnapshot};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub redis: RedisHealthResponse,
}

#[derive(Debug, Serialize)]
pub struct RedisHealthResponse {
    pub status: String,
    pub connected: bool,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub connections: ConnectionStats,
    pub notifications: NotificationStats,
    pub redis: RedisStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack: Option<AckStats>,
}

#[derive(Debug, Serialize)]
pub struct ConnectionStats {
    pub total_connections: usize,
    pub unique_users: usize,
    pub channels: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Serialize)]
pub struct NotificationStats {
    pub total_sent: u64,
    pub total_delivered: u64,
    pub total_failed: u64,
    pub user_notifications: u64,
    pub broadcast_notifications: u64,
    pub channel_notifications: u64,
}

#[derive(Debug, Serialize)]
pub struct RedisStats {
    pub status: String,
    pub connected: bool,
    pub circuit_breaker_state: String,
    pub circuit_breaker_failures: u32,
    pub reconnection_attempts: u32,
    pub total_reconnections: u32,
}

#[derive(Debug, Serialize)]
pub struct AckStats {
    pub enabled: bool,
    pub total_tracked: u64,
    pub total_acked: u64,
    pub total_expired: u64,
    pub pending_count: u64,
    pub ack_rate: f64,
    pub avg_latency_ms: u64,
}

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let redis_health = state.redis_health.stats();
    let is_healthy = redis_health.status == crate::redis::RedisHealthStatus::Healthy;

    Json(HealthResponse {
        status: if is_healthy { "healthy".to_string() } else { "degraded".to_string() },
        version: env!("CARGO_PKG_VERSION").to_string(),
        redis: RedisHealthResponse {
            status: redis_health.status.as_str().to_string(),
            connected: is_healthy,
        },
    })
}

pub async fn stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let conn_stats = state.connection_manager.stats();
    let dispatcher_stats = state.dispatcher.stats();
    let redis_health = state.redis_health.stats();
    let circuit_breaker = state.redis_circuit_breaker.stats();

    let circuit_state = match circuit_breaker.state {
        crate::redis::CircuitState::Closed => "closed",
        crate::redis::CircuitState::Open => "open",
        crate::redis::CircuitState::HalfOpen => "half_open",
    };

    // Get ACK stats if enabled
    let ack_stats = if state.ack_backend.is_enabled() {
        let ack = state.ack_backend.stats().await;
        Some(AckStats {
            enabled: true,
            total_tracked: ack.total_tracked,
            total_acked: ack.total_acked,
            total_expired: ack.total_expired,
            pending_count: ack.pending_count,
            ack_rate: ack.ack_rate,
            avg_latency_ms: ack.avg_latency_ms,
        })
    } else {
        None
    };

    Json(StatsResponse {
        connections: ConnectionStats {
            total_connections: conn_stats.total_connections,
            unique_users: conn_stats.unique_users,
            channels: conn_stats.channels,
        },
        notifications: NotificationStats {
            total_sent: dispatcher_stats.total_sent,
            total_delivered: dispatcher_stats.total_delivered,
            total_failed: dispatcher_stats.total_failed,
            user_notifications: dispatcher_stats.user_notifications,
            broadcast_notifications: dispatcher_stats.broadcast_notifications,
            channel_notifications: dispatcher_stats.channel_notifications,
        },
        redis: RedisStats {
            status: redis_health.status.as_str().to_string(),
            connected: redis_health.status == crate::redis::RedisHealthStatus::Healthy,
            circuit_breaker_state: circuit_state.to_string(),
            circuit_breaker_failures: circuit_breaker.failure_count,
            reconnection_attempts: redis_health.reconnection_attempts,
            total_reconnections: redis_health.total_reconnections,
        },
        ack: ack_stats,
    })
}

// ============================================================================
// Channel Info Endpoints
// ============================================================================

/// Response for listing all channels
#[derive(Debug, Serialize)]
pub struct ChannelListResponse {
    pub channels: Vec<ChannelInfo>,
    pub total_channels: usize,
}

/// Response for single channel info
#[derive(Debug, Serialize)]
pub struct ChannelDetailResponse {
    pub name: String,
    pub subscriber_count: usize,
}

/// Error response for channel endpoints
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

/// Response for user subscriptions
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

// ============================================================================
// Prometheus Metrics Endpoint
// ============================================================================

use axum::response::IntoResponse;
use crate::metrics;

/// GET /metrics - Prometheus metrics endpoint
pub async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    // Update current metrics from state before exporting
    update_metrics_from_state(&state).await;

    // Encode and return metrics
    match metrics::encode_metrics() {
        Ok(output) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
            output,
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to encode Prometheus metrics");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "text/plain")],
                format!("Failed to encode metrics: {}", e),
            )
        }
    }
}

/// Update Prometheus metrics from AppState
async fn update_metrics_from_state(state: &AppState) {
    // Connection metrics
    let conn_stats = state.connection_manager.stats();
    metrics::CONNECTIONS_TOTAL.set(conn_stats.total_connections as i64);
    metrics::USERS_CONNECTED.set(conn_stats.unique_users as i64);
    metrics::CHANNELS_ACTIVE.set(conn_stats.channels.len() as i64);

    // Update per-channel subscription counts
    for (channel, count) in &conn_stats.channels {
        metrics::CHANNEL_SUBSCRIPTIONS
            .with_label_values(&[channel])
            .set(*count as i64);
    }

    // Redis metrics
    let redis_health = state.redis_health.stats();
    let is_connected = redis_health.status == crate::redis::RedisHealthStatus::Healthy;
    metrics::REDIS_CONNECTION_STATUS.set(if is_connected { 1 } else { 0 });

    let circuit_breaker = state.redis_circuit_breaker.stats();
    let cb_state = match circuit_breaker.state {
        crate::redis::CircuitState::Closed => 0,
        crate::redis::CircuitState::Open => 1,
        crate::redis::CircuitState::HalfOpen => 2,
    };
    metrics::REDIS_CIRCUIT_BREAKER_STATE.set(cb_state);

    // Queue metrics
    if state.queue_backend.is_enabled() {
        let queue_stats = state.queue_backend.stats().await;
        metrics::QUEUE_SIZE_TOTAL.set(queue_stats.total_messages as i64);
        metrics::QUEUE_USERS_TOTAL.set(queue_stats.users_with_queue as i64);
    }

    // ACK metrics
    if state.ack_backend.is_enabled() {
        let ack_stats = state.ack_backend.stats().await;
        metrics::ACK_PENDING.set(ack_stats.pending_count as i64);
    }
}

// ============================================================================
// Template CRUD Endpoints
// ============================================================================

use crate::template::{
    CreateTemplateRequest, Template, TemplateError, TemplateListResponse, UpdateTemplateRequest,
};

/// Error response for template endpoints
#[derive(Debug, Serialize)]
pub struct TemplateErrorResponse {
    pub error: TemplateErrorInfo,
}

#[derive(Debug, Serialize)]
pub struct TemplateErrorInfo {
    pub code: String,
    pub message: String,
}

impl From<TemplateError> for (StatusCode, Json<TemplateErrorResponse>) {
    fn from(err: TemplateError) -> Self {
        let (status, code) = match &err {
            TemplateError::NotFound(_) => (StatusCode::NOT_FOUND, "TEMPLATE_NOT_FOUND"),
            TemplateError::AlreadyExists(_) => (StatusCode::CONFLICT, "TEMPLATE_EXISTS"),
            TemplateError::InvalidId(_) => (StatusCode::BAD_REQUEST, "INVALID_ID"),
            TemplateError::InvalidTemplate(_) => (StatusCode::BAD_REQUEST, "INVALID_TEMPLATE"),
            TemplateError::SubstitutionFailed(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "SUBSTITUTION_FAILED")
            }
        };

        (
            status,
            Json(TemplateErrorResponse {
                error: TemplateErrorInfo {
                    code: code.to_string(),
                    message: err.to_string(),
                },
            }),
        )
    }
}

/// POST /api/v1/templates - Create a new template
#[tracing::instrument(
    name = "http.create_template",
    skip(state, request),
    fields(template_id = %request.id)
)]
pub async fn create_template(
    State(state): State<AppState>,
    Json(request): Json<CreateTemplateRequest>,
) -> Result<(StatusCode, Json<Template>), (StatusCode, Json<TemplateErrorResponse>)> {
    let template: Template = request.into();

    match state.template_store.create(template) {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => Err(e.into()),
    }
}

/// GET /api/v1/templates - List all templates
#[tracing::instrument(name = "http.list_templates", skip(state))]
pub async fn list_templates(State(state): State<AppState>) -> Json<TemplateListResponse> {
    let templates = state.template_store.list();
    let total = templates.len();

    Json(TemplateListResponse { templates, total })
}

/// GET /api/v1/templates/:id - Get a specific template
#[tracing::instrument(name = "http.get_template", skip(state))]
pub async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Template>, (StatusCode, Json<TemplateErrorResponse>)> {
    match state.template_store.get(&id) {
        Ok(template) => Ok(Json(template)),
        Err(e) => Err(e.into()),
    }
}

/// PUT /api/v1/templates/:id - Update an existing template
#[tracing::instrument(name = "http.update_template", skip(state, request))]
pub async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateTemplateRequest>,
) -> Result<Json<Template>, (StatusCode, Json<TemplateErrorResponse>)> {
    match state.template_store.update(&id, request) {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => Err(e.into()),
    }
}

/// DELETE /api/v1/templates/:id - Delete a template
#[tracing::instrument(name = "http.delete_template", skip(state))]
pub async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<TemplateErrorResponse>)> {
    match state.template_store.delete(&id) {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(e.into()),
    }
}

// ============================================================================
// Cluster Endpoints
// ============================================================================

/// Response for cluster status
#[derive(Debug, Serialize)]
pub struct ClusterStatusResponse {
    pub enabled: bool,
    pub mode: String,
    pub server_id: String,
    pub local_connections: usize,
    pub local_users: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_connections: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_users: Option<usize>,
}

/// GET /api/v1/cluster/status - Get cluster status
#[tracing::instrument(name = "http.cluster_status", skip(state))]
pub async fn cluster_status(State(state): State<AppState>) -> Json<ClusterStatusResponse> {
    let enabled = state.session_store.is_enabled();
    let mode = if enabled { "distributed" } else { "standalone" };
    let server_id = state.session_store.server_id().to_string();

    let conn_stats = state.connection_manager.stats();

    // Get cluster-wide stats if enabled
    let (cluster_connections, cluster_users) = if enabled {
        match state.session_store.get_all_sessions().await {
            Ok(sessions) => {
                let unique_users: std::collections::HashSet<_> = sessions.iter()
                    .map(|s| s.user_id.clone())
                    .collect();
                (Some(sessions.len()), Some(unique_users.len()))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to get cluster sessions");
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    // Update cluster metrics
    crate::metrics::ClusterMetrics::set_enabled(enabled);
    if let Some(count) = cluster_connections {
        crate::metrics::ClusterMetrics::set_cluster_connections(count);
    }
    if let Some(count) = cluster_users {
        crate::metrics::ClusterMetrics::set_cluster_users(count);
    }

    Json(ClusterStatusResponse {
        enabled,
        mode: mode.to_string(),
        server_id,
        local_connections: conn_stats.total_connections,
        local_users: conn_stats.unique_users,
        cluster_connections,
        cluster_users,
    })
}

/// Response for user location lookup
#[derive(Debug, Serialize)]
pub struct UserLocationResponse {
    pub user_id: String,
    pub found: bool,
    pub sessions: Vec<UserSessionInfo>,
}

#[derive(Debug, Serialize)]
pub struct UserSessionInfo {
    pub connection_id: String,
    pub server_id: String,
    pub connected_at: i64,
    pub channels: Vec<String>,
}

/// GET /api/v1/cluster/users/:user_id - Locate user across cluster
#[tracing::instrument(name = "http.cluster_user_location", skip(state))]
pub async fn cluster_user_location(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Json<UserLocationResponse> {
    if !state.session_store.is_enabled() {
        // Fallback to local lookup
        let connections = state.connection_manager.get_user_connections(&user_id);
        let sessions: Vec<UserSessionInfo> = connections.iter().map(|c| UserSessionInfo {
            connection_id: c.id.to_string(),
            server_id: state.session_store.server_id().to_string(),
            connected_at: c.connected_at.timestamp(),
            channels: vec![], // Local mode doesn't track in session store
        }).collect();

        return Json(UserLocationResponse {
            user_id,
            found: !sessions.is_empty(),
            sessions,
        });
    }

    // Lookup in cluster session store
    match state.session_store.get_user_sessions(&user_id).await {
        Ok(sessions) => {
            let session_infos: Vec<UserSessionInfo> = sessions.iter().map(|s| UserSessionInfo {
                connection_id: s.connection_id.to_string(),
                server_id: s.server_id.clone(),
                connected_at: s.connected_at,
                channels: s.channels.clone(),
            }).collect();

            Json(UserLocationResponse {
                user_id,
                found: !session_infos.is_empty(),
                sessions: session_infos,
            })
        }
        Err(e) => {
            tracing::warn!(error = %e, user_id = %user_id, "Failed to lookup user in cluster");
            Json(UserLocationResponse {
                user_id,
                found: false,
                sessions: vec![],
            })
        }
    }
}

// ============================================================================
// Multi-Tenant Endpoints
// ============================================================================

/// Response for tenant list
#[derive(Debug, Serialize)]
pub struct TenantListResponse {
    pub enabled: bool,
    pub tenants: Vec<TenantInfo>,
    pub total: usize,
}

/// Response for single tenant stats
#[derive(Debug, Serialize)]
pub struct TenantStatsResponse {
    pub tenant_id: String,
    pub stats: TenantStatsSnapshot,
    pub connection_stats: TenantConnectionStatsResponse,
}

#[derive(Debug, Serialize)]
pub struct TenantConnectionStatsResponse {
    pub total_connections: usize,
    pub unique_users: usize,
}

/// GET /api/v1/tenants - List all active tenants
#[tracing::instrument(name = "http.list_tenants", skip(state))]
pub async fn list_tenants(State(state): State<AppState>) -> Json<TenantListResponse> {
    let enabled = state.tenant_manager.is_enabled();
    let tenants = state.tenant_manager.list_active_tenants();
    let total = tenants.len();

    Json(TenantListResponse {
        enabled,
        tenants,
        total,
    })
}

/// GET /api/v1/tenants/:tenant_id - Get tenant stats
#[tracing::instrument(name = "http.get_tenant_stats", skip(state))]
pub async fn get_tenant_stats(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantStatsResponse>, (StatusCode, Json<ChannelErrorResponse>)> {
    let stats = state.tenant_manager.get_stats(&tenant_id);
    let conn_stats = state.connection_manager.tenant_stats(&tenant_id);

    // If tenant has no activity, return 404
    if stats.total_connections == 0 && conn_stats.total_connections == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ChannelErrorResponse {
                error: ChannelError {
                    code: "TENANT_NOT_FOUND".to_string(),
                    message: format!("Tenant '{}' has no recorded activity", tenant_id),
                },
            }),
        ));
    }

    Ok(Json(TenantStatsResponse {
        tenant_id,
        stats,
        connection_stats: TenantConnectionStatsResponse {
            total_connections: conn_stats.total_connections,
            unique_users: conn_stats.unique_users,
        },
    }))
}

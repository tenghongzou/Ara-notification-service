//! Multi-tenant management endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::server::AppState;
use crate::tenant::{TenantInfo, TenantStatsSnapshot};

use super::connection::ChannelErrorResponse;

#[derive(Debug, Serialize)]
pub struct TenantListResponse {
    pub enabled: bool,
    pub tenants: Vec<TenantInfo>,
    pub total: usize,
}

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
    use super::connection::ChannelError;

    let stats = state.tenant_manager.get_stats(&tenant_id);
    let conn_stats = state.connection_manager.tenant_stats(&tenant_id);

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

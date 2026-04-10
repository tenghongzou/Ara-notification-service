//! Multi-tenant management endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Serialize;

use crate::server::middleware::RequestTenantContext;
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

/// GET /api/v1/tenants - List active tenants (scoped to caller's tenant when multi-tenancy is enabled)
#[tracing::instrument(name = "http.list_tenants", skip(state, tenant_ctx))]
pub async fn list_tenants(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
) -> Json<TenantListResponse> {
    let enabled = state.tenant_manager.is_enabled();
    let all_tenants = state.tenant_manager.list_active_tenants();

    // When tenant context is present, only show the caller's own tenant
    let tenants: Vec<TenantInfo> = match tenant_ctx.as_ref() {
        Some(t) => {
            let tid = t.0.tenant_id();
            all_tenants
                .into_iter()
                .filter(|info| info.tenant_id == tid)
                .collect()
        }
        None => all_tenants,
    };
    let total = tenants.len();

    Json(TenantListResponse {
        enabled,
        tenants,
        total,
    })
}

/// GET /api/v1/tenants/:tenant_id - Get tenant stats (scoped to caller's tenant)
#[tracing::instrument(name = "http.get_tenant_stats", skip(state, tenant_ctx))]
pub async fn get_tenant_stats(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantStatsResponse>, (StatusCode, Json<ChannelErrorResponse>)> {
    use super::connection::ChannelError;

    // When tenant context is present, only allow querying the caller's own tenant
    if let Some(ref t) = tenant_ctx {
        if t.0.tenant_id() != tenant_id {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ChannelErrorResponse {
                    error: ChannelError {
                        code: "TENANT_ACCESS_DENIED".to_string(),
                        message: "Cannot access another tenant's stats".to_string(),
                    },
                }),
            ));
        }
    }

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

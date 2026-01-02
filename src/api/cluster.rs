//! Cluster management endpoints.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use crate::server::AppState;

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
        let connections = state.connection_manager.get_user_connections(&user_id);
        let sessions: Vec<UserSessionInfo> = connections.iter().map(|c| UserSessionInfo {
            connection_id: c.id.to_string(),
            server_id: state.session_store.server_id().to_string(),
            connected_at: c.connected_at.timestamp(),
            channels: vec![],
        }).collect();

        return Json(UserLocationResponse {
            user_id,
            found: !sessions.is_empty(),
            sessions,
        });
    }

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

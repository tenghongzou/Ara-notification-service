use axum::{extract::State, Json};
use serde::Serialize;

use crate::server::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub connections: ConnectionStats,
    pub notifications: NotificationStats,
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

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

pub async fn stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let conn_stats = state.connection_manager.stats();
    let dispatcher_stats = state.dispatcher.stats();

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
    })
}

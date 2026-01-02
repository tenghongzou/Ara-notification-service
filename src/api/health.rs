//! Health check and statistics endpoints.

use axum::{
    extract::State,
    Json,
};
use serde::Serialize;

use crate::server::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub redis: RedisHealthResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres: Option<PostgresHealthResponse>,
    pub connections: ConnectionHealthResponse,
    pub queue: QueueHealthResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterHealthResponse>,
}

#[derive(Debug, Serialize)]
pub struct RedisHealthResponse {
    pub status: String,
    pub connected: bool,
}

#[derive(Debug, Serialize)]
pub struct PostgresHealthResponse {
    pub status: String,
    pub connected: bool,
    pub pool_size: u32,
    pub idle_connections: u32,
}

#[derive(Debug, Serialize)]
pub struct ConnectionHealthResponse {
    pub total: usize,
    pub unique_users: usize,
    pub channels_count: usize,
}

#[derive(Debug, Serialize)]
pub struct QueueHealthResponse {
    pub enabled: bool,
    pub backend: String,
    pub total_messages: usize,
    pub users_with_queue: usize,
}

#[derive(Debug, Serialize)]
pub struct ClusterHealthResponse {
    pub enabled: bool,
    pub server_id: String,
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
    let is_redis_healthy = redis_health.status == crate::redis::RedisHealthStatus::Healthy;

    let uptime_seconds = state.start_time.elapsed().as_secs();
    let conn_stats = state.connection_manager.stats();
    let queue_stats = state.queue_backend.stats().await;

    let postgres = if let Some(ref pool) = state.postgres_pool {
        let inner_pool = pool.pool();
        Some(PostgresHealthResponse {
            status: "connected".to_string(),
            connected: pool.is_available(),
            pool_size: inner_pool.size(),
            idle_connections: inner_pool.num_idle() as u32,
        })
    } else {
        None
    };

    let cluster = if state.settings.cluster.enabled {
        Some(ClusterHealthResponse {
            enabled: true,
            server_id: state.settings.cluster.server_id.clone(),
        })
    } else {
        None
    };

    let status = if is_redis_healthy { "healthy" } else { "degraded" };

    Json(HealthResponse {
        status: status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds,
        redis: RedisHealthResponse {
            status: redis_health.status.as_str().to_string(),
            connected: is_redis_healthy,
        },
        postgres,
        connections: ConnectionHealthResponse {
            total: conn_stats.total_connections,
            unique_users: conn_stats.unique_users,
            channels_count: conn_stats.channels.len(),
        },
        queue: QueueHealthResponse {
            enabled: state.queue_backend.is_enabled(),
            backend: state.settings.queue.backend.clone(),
            total_messages: queue_stats.total_messages,
            users_with_queue: queue_stats.users_with_queue,
        },
        cluster,
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

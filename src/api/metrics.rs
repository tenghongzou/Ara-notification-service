//! Prometheus metrics endpoint.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};

use crate::metrics;
use crate::server::AppState;

/// GET /metrics - Prometheus metrics endpoint
pub async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    update_metrics_from_state(&state).await;

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

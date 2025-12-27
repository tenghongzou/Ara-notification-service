//! Prometheus metrics for the notification service.
//!
//! This module provides comprehensive metrics for monitoring the notification service:
//! - Connection metrics (active connections, per-user counts)
//! - Message metrics (sent, delivered, failed by target type)
//! - Latency metrics (message delivery, ACK latency)
//! - Redis health metrics
//! - Queue metrics
//! - Rate limiting metrics

use lazy_static::lazy_static;
use prometheus::{
    register_histogram, register_histogram_vec, register_int_counter, register_int_counter_vec,
    register_int_gauge, register_int_gauge_vec, Encoder, Histogram, HistogramVec, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec, TextEncoder,
};

/// Prefix for all metrics
const METRIC_PREFIX: &str = "ara";

lazy_static! {
    // ============================================================================
    // Connection Metrics
    // ============================================================================

    /// Total number of active WebSocket connections
    pub static ref CONNECTIONS_TOTAL: IntGauge = register_int_gauge!(
        format!("{}_connections_total", METRIC_PREFIX),
        "Total number of active WebSocket connections"
    ).unwrap();

    /// Number of unique connected users
    pub static ref USERS_CONNECTED: IntGauge = register_int_gauge!(
        format!("{}_users_connected", METRIC_PREFIX),
        "Number of unique connected users"
    ).unwrap();

    /// Connections per user (for detecting connection hoarding)
    pub static ref CONNECTIONS_PER_USER: Histogram = register_histogram!(
        format!("{}_connections_per_user", METRIC_PREFIX),
        "Distribution of connections per user",
        vec![1.0, 2.0, 3.0, 4.0, 5.0, 10.0]
    ).unwrap();

    /// Number of active channel subscriptions
    pub static ref CHANNEL_SUBSCRIPTIONS: IntGaugeVec = register_int_gauge_vec!(
        format!("{}_channel_subscriptions", METRIC_PREFIX),
        "Number of subscriptions per channel",
        &["channel"]
    ).unwrap();

    /// Total channels with subscribers
    pub static ref CHANNELS_ACTIVE: IntGauge = register_int_gauge!(
        format!("{}_channels_active", METRIC_PREFIX),
        "Total number of channels with at least one subscriber"
    ).unwrap();

    // ============================================================================
    // Message Metrics
    // ============================================================================

    /// Total messages sent by target type
    pub static ref MESSAGES_SENT_TOTAL: IntCounterVec = register_int_counter_vec!(
        format!("{}_messages_sent_total", METRIC_PREFIX),
        "Total messages sent",
        &["target"]
    ).unwrap();

    /// Total messages delivered (connection count)
    pub static ref MESSAGES_DELIVERED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_messages_delivered_total", METRIC_PREFIX),
        "Total messages successfully delivered to connections"
    ).unwrap();

    /// Total message delivery failures
    pub static ref MESSAGES_FAILED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_messages_failed_total", METRIC_PREFIX),
        "Total message delivery failures"
    ).unwrap();

    /// Message delivery latency (time from dispatch to connection send)
    pub static ref MESSAGE_DELIVERY_LATENCY: Histogram = register_histogram!(
        format!("{}_message_delivery_latency_seconds", METRIC_PREFIX),
        "Message delivery latency in seconds",
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
    ).unwrap();

    // ============================================================================
    // Redis Metrics
    // ============================================================================

    /// Redis connection status (1 = connected, 0 = disconnected)
    pub static ref REDIS_CONNECTION_STATUS: IntGauge = register_int_gauge!(
        format!("{}_redis_connection_status", METRIC_PREFIX),
        "Redis connection status (1=connected, 0=disconnected)"
    ).unwrap();

    /// Redis circuit breaker state (0=closed, 1=open, 2=half-open)
    pub static ref REDIS_CIRCUIT_BREAKER_STATE: IntGauge = register_int_gauge!(
        format!("{}_redis_circuit_breaker_state", METRIC_PREFIX),
        "Redis circuit breaker state (0=closed, 1=open, 2=half-open)"
    ).unwrap();

    /// Total Redis reconnection attempts
    pub static ref REDIS_RECONNECTIONS_TOTAL: IntCounter = register_int_counter!(
        format!("{}_redis_reconnections_total", METRIC_PREFIX),
        "Total Redis reconnection attempts"
    ).unwrap();

    /// Redis pub/sub messages received
    pub static ref REDIS_MESSAGES_RECEIVED: IntCounter = register_int_counter!(
        format!("{}_redis_messages_received_total", METRIC_PREFIX),
        "Total messages received from Redis pub/sub"
    ).unwrap();

    // ============================================================================
    // Queue Metrics
    // ============================================================================

    /// Total messages currently queued
    pub static ref QUEUE_SIZE_TOTAL: IntGauge = register_int_gauge!(
        format!("{}_queue_size_total", METRIC_PREFIX),
        "Total messages currently in queue"
    ).unwrap();

    /// Number of users with queued messages
    pub static ref QUEUE_USERS_TOTAL: IntGauge = register_int_gauge!(
        format!("{}_queue_users_total", METRIC_PREFIX),
        "Number of users with queued messages"
    ).unwrap();

    /// Messages enqueued (for offline users)
    pub static ref QUEUE_ENQUEUED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_queue_enqueued_total", METRIC_PREFIX),
        "Total messages enqueued for offline users"
    ).unwrap();

    /// Messages replayed on reconnect
    pub static ref QUEUE_REPLAYED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_queue_replayed_total", METRIC_PREFIX),
        "Total messages replayed on user reconnect"
    ).unwrap();

    /// Messages expired from queue
    pub static ref QUEUE_EXPIRED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_queue_expired_total", METRIC_PREFIX),
        "Total messages expired from queue"
    ).unwrap();

    /// Messages dropped due to queue full
    pub static ref QUEUE_DROPPED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_queue_dropped_total", METRIC_PREFIX),
        "Total messages dropped due to queue being full"
    ).unwrap();

    // ============================================================================
    // Rate Limiting Metrics
    // ============================================================================

    /// Requests allowed by rate limiter
    pub static ref RATELIMIT_ALLOWED_TOTAL: IntCounterVec = register_int_counter_vec!(
        format!("{}_ratelimit_allowed_total", METRIC_PREFIX),
        "Total requests allowed by rate limiter",
        &["type"]
    ).unwrap();

    /// Requests denied by rate limiter
    pub static ref RATELIMIT_DENIED_TOTAL: IntCounterVec = register_int_counter_vec!(
        format!("{}_ratelimit_denied_total", METRIC_PREFIX),
        "Total requests denied by rate limiter",
        &["type"]
    ).unwrap();

    // ============================================================================
    // ACK Metrics
    // ============================================================================

    /// Total notifications tracked for ACK
    pub static ref ACK_TRACKED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_ack_tracked_total", METRIC_PREFIX),
        "Total notifications tracked for ACK"
    ).unwrap();

    /// Total ACKs received
    pub static ref ACK_RECEIVED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_ack_received_total", METRIC_PREFIX),
        "Total ACKs received from clients"
    ).unwrap();

    /// Total ACKs expired (not received in time)
    pub static ref ACK_EXPIRED_TOTAL: IntCounter = register_int_counter!(
        format!("{}_ack_expired_total", METRIC_PREFIX),
        "Total ACKs that expired without being received"
    ).unwrap();

    /// Current pending ACKs
    pub static ref ACK_PENDING: IntGauge = register_int_gauge!(
        format!("{}_ack_pending", METRIC_PREFIX),
        "Current number of pending ACKs"
    ).unwrap();

    /// ACK latency histogram
    pub static ref ACK_LATENCY: Histogram = register_histogram!(
        format!("{}_ack_latency_seconds", METRIC_PREFIX),
        "ACK latency in seconds (time from send to ACK)",
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    ).unwrap();

    // ============================================================================
    // HTTP API Metrics
    // ============================================================================

    /// HTTP request counter by method and path
    pub static ref HTTP_REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        format!("{}_http_requests_total", METRIC_PREFIX),
        "Total HTTP requests",
        &["method", "path", "status"]
    ).unwrap();

    /// HTTP request latency
    pub static ref HTTP_REQUEST_LATENCY: HistogramVec = register_histogram_vec!(
        format!("{}_http_request_latency_seconds", METRIC_PREFIX),
        "HTTP request latency in seconds",
        &["method", "path"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
    ).unwrap();

    // ============================================================================
    // WebSocket Metrics
    // ============================================================================

    /// WebSocket connections opened
    pub static ref WS_CONNECTIONS_OPENED: IntCounter = register_int_counter!(
        format!("{}_ws_connections_opened_total", METRIC_PREFIX),
        "Total WebSocket connections opened"
    ).unwrap();

    /// WebSocket connections closed
    pub static ref WS_CONNECTIONS_CLOSED: IntCounter = register_int_counter!(
        format!("{}_ws_connections_closed_total", METRIC_PREFIX),
        "Total WebSocket connections closed"
    ).unwrap();

    /// WebSocket messages received from clients
    pub static ref WS_MESSAGES_RECEIVED: IntCounterVec = register_int_counter_vec!(
        format!("{}_ws_messages_received_total", METRIC_PREFIX),
        "Total WebSocket messages received from clients",
        &["type"]
    ).unwrap();

    /// WebSocket connection duration
    pub static ref WS_CONNECTION_DURATION: Histogram = register_histogram!(
        format!("{}_ws_connection_duration_seconds", METRIC_PREFIX),
        "WebSocket connection duration in seconds",
        vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0]
    ).unwrap();

    // ============================================================================
    // Batch API Metrics
    // ============================================================================

    /// Batch requests processed
    pub static ref BATCH_REQUESTS_TOTAL: IntCounter = register_int_counter!(
        format!("{}_batch_requests_total", METRIC_PREFIX),
        "Total batch notification requests"
    ).unwrap();

    /// Notifications per batch
    pub static ref BATCH_SIZE: Histogram = register_histogram!(
        format!("{}_batch_size", METRIC_PREFIX),
        "Number of notifications per batch request",
        vec![1.0, 5.0, 10.0, 25.0, 50.0, 75.0, 100.0]
    ).unwrap();

    // ============================================================================
    // Process & Memory Metrics
    // ============================================================================

    /// Process memory usage (resident set size) in bytes
    pub static ref PROCESS_MEMORY_BYTES: IntGauge = register_int_gauge!(
        format!("{}_process_memory_bytes", METRIC_PREFIX),
        "Process memory usage (RSS) in bytes"
    ).unwrap();

    /// Estimated connection manager memory in bytes
    pub static ref CONNECTION_MANAGER_MEMORY_BYTES: IntGauge = register_int_gauge!(
        format!("{}_connection_manager_memory_bytes", METRIC_PREFIX),
        "Estimated memory used by connection manager"
    ).unwrap();

    /// Estimated queue memory in bytes
    pub static ref QUEUE_MEMORY_BYTES: IntGauge = register_int_gauge!(
        format!("{}_queue_memory_bytes", METRIC_PREFIX),
        "Estimated memory used by message queue"
    ).unwrap();

    /// Heartbeat round duration in milliseconds
    pub static ref HEARTBEAT_DURATION_MS: Histogram = register_histogram!(
        format!("{}_heartbeat_duration_ms", METRIC_PREFIX),
        "Heartbeat round duration in milliseconds",
        vec![10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0]
    ).unwrap();

    /// Heartbeat timeouts per round
    pub static ref HEARTBEAT_TIMEOUTS: IntCounter = register_int_counter!(
        format!("{}_heartbeat_timeouts_total", METRIC_PREFIX),
        "Total heartbeat send timeouts"
    ).unwrap();

    // ============================================================================
    // Backend Metrics
    // ============================================================================

    /// Backend operation latency
    pub static ref BACKEND_OPERATION_LATENCY: HistogramVec = register_histogram_vec!(
        format!("{}_backend_operation_latency_seconds", METRIC_PREFIX),
        "Backend operation latency in seconds",
        &["backend", "operation"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
    ).unwrap();

    /// Backend operation errors
    pub static ref BACKEND_ERRORS_TOTAL: IntCounterVec = register_int_counter_vec!(
        format!("{}_backend_errors_total", METRIC_PREFIX),
        "Total backend operation errors",
        &["backend", "operation"]
    ).unwrap();

    // ============================================================================
    // Cluster Metrics
    // ============================================================================

    /// Cluster mode enabled (1=enabled, 0=disabled)
    pub static ref CLUSTER_ENABLED: IntGauge = register_int_gauge!(
        format!("{}_cluster_enabled", METRIC_PREFIX),
        "Cluster mode enabled (1=enabled, 0=disabled)"
    ).unwrap();

    /// Cluster-wide total connections (across all servers)
    pub static ref CLUSTER_CONNECTIONS_TOTAL: IntGauge = register_int_gauge!(
        format!("{}_cluster_connections_total", METRIC_PREFIX),
        "Cluster-wide total connections"
    ).unwrap();

    /// Cluster-wide unique users (across all servers)
    pub static ref CLUSTER_USERS_TOTAL: IntGauge = register_int_gauge!(
        format!("{}_cluster_users_total", METRIC_PREFIX),
        "Cluster-wide unique users"
    ).unwrap();

    /// Sessions refreshed during heartbeat
    pub static ref CLUSTER_SESSIONS_REFRESHED: IntCounter = register_int_counter!(
        format!("{}_cluster_sessions_refreshed_total", METRIC_PREFIX),
        "Total cluster sessions refreshed"
    ).unwrap();

    /// Routed messages published (to other servers)
    pub static ref CLUSTER_MESSAGES_ROUTED: IntCounter = register_int_counter!(
        format!("{}_cluster_messages_routed_total", METRIC_PREFIX),
        "Total messages routed to other servers"
    ).unwrap();

    /// Routed messages received (from other servers)
    pub static ref CLUSTER_MESSAGES_RECEIVED: IntCounter = register_int_counter!(
        format!("{}_cluster_messages_received_total", METRIC_PREFIX),
        "Total messages received from other servers"
    ).unwrap();
}

/// Encode all metrics to Prometheus text format
pub fn encode_metrics() -> Result<String, prometheus::Error> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer).unwrap_or_default())
}

/// Helper struct for recording message metrics
pub struct MessageMetrics;

impl MessageMetrics {
    /// Record a message sent to a user
    pub fn record_user_sent() {
        MESSAGES_SENT_TOTAL.with_label_values(&["user"]).inc();
    }

    /// Record a message sent to multiple users
    pub fn record_users_sent() {
        MESSAGES_SENT_TOTAL.with_label_values(&["users"]).inc();
    }

    /// Record a broadcast message
    pub fn record_broadcast_sent() {
        MESSAGES_SENT_TOTAL.with_label_values(&["broadcast"]).inc();
    }

    /// Record a channel message
    pub fn record_channel_sent() {
        MESSAGES_SENT_TOTAL.with_label_values(&["channel"]).inc();
    }

    /// Record a multi-channel message
    pub fn record_channels_sent() {
        MESSAGES_SENT_TOTAL.with_label_values(&["channels"]).inc();
    }

    /// Record successful deliveries
    pub fn record_delivered(count: u64) {
        MESSAGES_DELIVERED_TOTAL.inc_by(count);
    }

    /// Record failed deliveries
    pub fn record_failed(count: u64) {
        MESSAGES_FAILED_TOTAL.inc_by(count);
    }
}

/// Helper struct for recording rate limit metrics
pub struct RateLimitMetrics;

impl RateLimitMetrics {
    /// Record an allowed HTTP request
    pub fn record_http_allowed() {
        RATELIMIT_ALLOWED_TOTAL.with_label_values(&["http"]).inc();
    }

    /// Record a denied HTTP request
    pub fn record_http_denied() {
        RATELIMIT_DENIED_TOTAL.with_label_values(&["http"]).inc();
    }

    /// Record an allowed WebSocket connection
    pub fn record_ws_allowed() {
        RATELIMIT_ALLOWED_TOTAL.with_label_values(&["ws"]).inc();
    }

    /// Record a denied WebSocket connection
    pub fn record_ws_denied() {
        RATELIMIT_DENIED_TOTAL.with_label_values(&["ws"]).inc();
    }
}

/// Helper struct for memory metrics
pub struct MemoryMetrics;

impl MemoryMetrics {
    /// Update process memory metric (call periodically)
    pub fn update_process_memory() {
        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<i64>() {
                                PROCESS_MEMORY_BYTES.set(kb * 1024);
                                return;
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // On Windows, we can use GetProcessMemoryInfo but that requires winapi
            // For now, just set to 0 to indicate not available
            // In production, consider using the `sysinfo` crate
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, we could use mach APIs
            // For now, just set to 0 to indicate not available
        }
    }

    /// Update connection manager memory estimate
    pub fn update_connection_manager_memory(connections: usize, subscriptions: usize) {
        // Rough estimate: each connection handle ~500 bytes, each subscription ~100 bytes
        const CONNECTION_SIZE: usize = 500;
        const SUBSCRIPTION_SIZE: usize = 100;
        let estimated = (connections * CONNECTION_SIZE + subscriptions * SUBSCRIPTION_SIZE) as i64;
        CONNECTION_MANAGER_MEMORY_BYTES.set(estimated);
    }

    /// Update queue memory estimate
    pub fn update_queue_memory(messages: usize, avg_message_size: usize) {
        // Estimate based on message count and average size
        let estimated = (messages * avg_message_size) as i64;
        QUEUE_MEMORY_BYTES.set(estimated);
    }
}

/// Helper struct for heartbeat metrics
pub struct HeartbeatMetrics;

impl HeartbeatMetrics {
    /// Record heartbeat round duration
    pub fn record_duration_ms(duration_ms: u64) {
        HEARTBEAT_DURATION_MS.observe(duration_ms as f64);
    }

    /// Record heartbeat timeouts
    pub fn record_timeouts(count: u64) {
        HEARTBEAT_TIMEOUTS.inc_by(count);
    }
}

/// Helper struct for backend metrics
pub struct BackendMetrics;

impl BackendMetrics {
    /// Record backend operation latency
    pub fn record_latency(backend: &str, operation: &str, latency_secs: f64) {
        BACKEND_OPERATION_LATENCY
            .with_label_values(&[backend, operation])
            .observe(latency_secs);
    }

    /// Record backend error
    pub fn record_error(backend: &str, operation: &str) {
        BACKEND_ERRORS_TOTAL
            .with_label_values(&[backend, operation])
            .inc();
    }
}

/// Helper struct for cluster metrics
pub struct ClusterMetrics;

impl ClusterMetrics {
    /// Set cluster enabled status
    pub fn set_enabled(enabled: bool) {
        CLUSTER_ENABLED.set(if enabled { 1 } else { 0 });
    }

    /// Update cluster-wide connection count
    pub fn set_cluster_connections(count: usize) {
        CLUSTER_CONNECTIONS_TOTAL.set(count as i64);
    }

    /// Update cluster-wide user count
    pub fn set_cluster_users(count: usize) {
        CLUSTER_USERS_TOTAL.set(count as i64);
    }

    /// Record sessions refreshed
    pub fn record_sessions_refreshed(count: usize) {
        CLUSTER_SESSIONS_REFRESHED.inc_by(count as u64);
    }

    /// Record message routed to another server
    pub fn record_message_routed() {
        CLUSTER_MESSAGES_ROUTED.inc();
    }

    /// Record message received from another server
    pub fn record_message_received() {
        CLUSTER_MESSAGES_RECEIVED.inc();
    }
}

/// Helper struct for recording WebSocket message metrics
pub struct WsMessageMetrics;

impl WsMessageMetrics {
    /// Record a subscribe message
    pub fn record_subscribe() {
        WS_MESSAGES_RECEIVED.with_label_values(&["subscribe"]).inc();
    }

    /// Record an unsubscribe message
    pub fn record_unsubscribe() {
        WS_MESSAGES_RECEIVED.with_label_values(&["unsubscribe"]).inc();
    }

    /// Record a ping message
    pub fn record_ping() {
        WS_MESSAGES_RECEIVED.with_label_values(&["ping"]).inc();
    }

    /// Record an ACK message
    pub fn record_ack() {
        WS_MESSAGES_RECEIVED.with_label_values(&["ack"]).inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_metrics() {
        // Initialize some metrics first (lazy_static requires first access)
        CONNECTIONS_TOTAL.set(1);

        // Verify encoding doesn't panic and contains expected prefix
        let result = encode_metrics();
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("ara_connections_total"));
    }

    #[test]
    fn test_message_metrics() {
        MessageMetrics::record_user_sent();
        MessageMetrics::record_broadcast_sent();
        MessageMetrics::record_channel_sent();
        MessageMetrics::record_delivered(5);
        MessageMetrics::record_failed(1);
        // Just verify no panics
    }

    #[test]
    fn test_rate_limit_metrics() {
        RateLimitMetrics::record_http_allowed();
        RateLimitMetrics::record_http_denied();
        RateLimitMetrics::record_ws_allowed();
        RateLimitMetrics::record_ws_denied();
        // Just verify no panics
    }

    #[test]
    fn test_ws_message_metrics() {
        WsMessageMetrics::record_subscribe();
        WsMessageMetrics::record_unsubscribe();
        WsMessageMetrics::record_ping();
        WsMessageMetrics::record_ack();
        // Just verify no panics
    }

    #[test]
    fn test_connection_metrics() {
        CONNECTIONS_TOTAL.set(100);
        USERS_CONNECTED.set(50);
        CONNECTIONS_PER_USER.observe(2.0);
        CHANNELS_ACTIVE.set(10);
        // Just verify no panics
    }

    #[test]
    fn test_redis_metrics() {
        REDIS_CONNECTION_STATUS.set(1);
        REDIS_CIRCUIT_BREAKER_STATE.set(0);
        REDIS_RECONNECTIONS_TOTAL.inc();
        REDIS_MESSAGES_RECEIVED.inc();
        // Just verify no panics
    }

    #[test]
    fn test_queue_metrics() {
        QUEUE_SIZE_TOTAL.set(50);
        QUEUE_USERS_TOTAL.set(10);
        QUEUE_ENQUEUED_TOTAL.inc();
        QUEUE_REPLAYED_TOTAL.inc();
        QUEUE_EXPIRED_TOTAL.inc();
        QUEUE_DROPPED_TOTAL.inc();
        // Just verify no panics
    }

    #[test]
    fn test_ack_metrics() {
        ACK_TRACKED_TOTAL.inc();
        ACK_RECEIVED_TOTAL.inc();
        ACK_EXPIRED_TOTAL.inc();
        ACK_PENDING.set(5);
        ACK_LATENCY.observe(0.1);
        // Just verify no panics
    }

    #[test]
    fn test_cluster_metrics() {
        ClusterMetrics::set_enabled(true);
        ClusterMetrics::set_cluster_connections(100);
        ClusterMetrics::set_cluster_users(50);
        ClusterMetrics::record_sessions_refreshed(10);
        ClusterMetrics::record_message_routed();
        ClusterMetrics::record_message_received();
        // Just verify no panics
    }
}

//! Metrics helper structs for convenient metric recording

use prometheus::{Encoder, TextEncoder};

use super::{
    ACK_EXPIRED_TOTAL, ACK_LATENCY, ACK_PENDING, ACK_RECEIVED_TOTAL, ACK_TRACKED_TOTAL,
    BACKEND_ERRORS_TOTAL, BACKEND_OPERATION_LATENCY, CLUSTER_CONNECTIONS_TOTAL, CLUSTER_ENABLED,
    CLUSTER_MESSAGES_RECEIVED, CLUSTER_MESSAGES_ROUTED, CLUSTER_SESSIONS_REFRESHED,
    CLUSTER_USERS_TOTAL, CONNECTION_MANAGER_MEMORY_BYTES, HEARTBEAT_DURATION_MS,
    HEARTBEAT_TIMEOUTS, MESSAGES_DELIVERED_TOTAL, MESSAGES_FAILED_TOTAL, MESSAGES_SENT_TOTAL,
    PROCESS_MEMORY_BYTES, QUEUE_MEMORY_BYTES, RATELIMIT_ALLOWED_TOTAL, RATELIMIT_DENIED_TOTAL,
    WS_MESSAGES_RECEIVED,
};

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
        WS_MESSAGES_RECEIVED
            .with_label_values(&["unsubscribe"])
            .inc();
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

/// Helper struct for ACK metrics
pub struct AckMetrics;

impl AckMetrics {
    /// Record a tracked notification
    pub fn record_tracked() {
        ACK_TRACKED_TOTAL.inc();
    }

    /// Record an ACK received
    pub fn record_received() {
        ACK_RECEIVED_TOTAL.inc();
    }

    /// Record an expired ACK
    pub fn record_expired() {
        ACK_EXPIRED_TOTAL.inc();
    }

    /// Set pending ACK count
    pub fn set_pending(count: i64) {
        ACK_PENDING.set(count);
    }

    /// Record ACK latency
    pub fn record_latency(latency_secs: f64) {
        ACK_LATENCY.observe(latency_secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

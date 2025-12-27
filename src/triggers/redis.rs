use std::sync::Arc;

use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::config::RedisConfig;
use crate::notification::{NotificationBuilder, NotificationDispatcher, NotificationTarget, Priority};
use crate::redis::{
    BackoffConfig, CircuitBreaker, CircuitBreakerConfig, CircuitState,
    ExponentialBackoff, RedisHealth,
};

/// Message format received from Redis Pub/Sub
#[derive(Debug, Deserialize)]
pub struct RedisNotificationMessage {
    /// Target type: "user", "users", "broadcast", "channel", "channels"
    #[serde(rename = "type")]
    pub target_type: String,
    /// Target value (user_id, channel name, or list)
    pub target: Option<RedisTarget>,
    /// Event data
    pub event: RedisEventData,
}

/// Target specification in Redis message
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RedisTarget {
    Single(String),
    Multiple(Vec<String>),
}

/// Event data in Redis message
#[derive(Debug, Deserialize)]
pub struct RedisEventData {
    /// Event type
    pub event_type: String,
    /// Event payload
    pub payload: serde_json::Value,
    /// Priority (optional)
    #[serde(default)]
    pub priority: Priority,
    /// TTL in seconds (optional)
    pub ttl: Option<u32>,
    /// Correlation ID (optional)
    pub correlation_id: Option<String>,
}

/// Resilient Redis Pub/Sub subscriber with circuit breaker and exponential backoff
pub struct RedisSubscriber {
    config: RedisConfig,
    dispatcher: Arc<NotificationDispatcher>,
    shutdown: broadcast::Sender<()>,
    circuit_breaker: Arc<CircuitBreaker>,
    health: Arc<RedisHealth>,
}

impl RedisSubscriber {
    /// Create a new Redis subscriber
    pub fn new(
        config: RedisConfig,
        dispatcher: Arc<NotificationDispatcher>,
        circuit_breaker: Arc<CircuitBreaker>,
        health: Arc<RedisHealth>,
    ) -> Self {
        let (shutdown, _) = broadcast::channel(1);
        Self {
            config,
            dispatcher,
            shutdown,
            circuit_breaker,
            health,
        }
    }

    /// Create a new Redis subscriber with default circuit breaker and health
    pub fn with_defaults(config: RedisConfig, dispatcher: Arc<NotificationDispatcher>) -> Self {
        let cb_config = CircuitBreakerConfig {
            failure_threshold: config.circuit_breaker_failure_threshold,
            success_threshold: config.circuit_breaker_success_threshold,
            reset_timeout_ms: config.circuit_breaker_reset_timeout_seconds * 1000,
        };
        let circuit_breaker = Arc::new(CircuitBreaker::with_config(cb_config));
        let health = Arc::new(RedisHealth::new());

        Self::new(config, dispatcher, circuit_breaker, health)
    }

    /// Get a shutdown signal sender
    pub fn shutdown_signal(&self) -> broadcast::Sender<()> {
        self.shutdown.clone()
    }

    /// Get the circuit breaker reference
    pub fn circuit_breaker(&self) -> Arc<CircuitBreaker> {
        Arc::clone(&self.circuit_breaker)
    }

    /// Get the health tracker reference
    pub fn health(&self) -> Arc<RedisHealth> {
        Arc::clone(&self.health)
    }

    /// Start the Redis subscriber loop with resilience
    pub async fn start(&self) -> anyhow::Result<()> {
        let channels = self.get_channels();
        if channels.is_empty() {
            tracing::info!("No Redis channels configured, skipping Redis subscriber");
            return Ok(());
        }

        tracing::info!(channels = ?channels, "Starting resilient Redis subscriber");

        // Create backoff configuration
        let backoff_config = BackoffConfig {
            initial_delay_ms: self.config.backoff_initial_delay_ms,
            max_delay_ms: self.config.backoff_max_delay_ms,
            multiplier: 2.0,
            jitter_factor: 0.1,
        };
        let mut backoff = ExponentialBackoff::with_config(backoff_config);

        loop {
            // Check circuit breaker state
            match self.circuit_breaker.state() {
                CircuitState::Open => {
                    self.health.set_circuit_open();
                    tracing::warn!("Circuit breaker is open, waiting for reset timeout");

                    // Wait for circuit breaker to transition to half-open
                    let wait_time = std::time::Duration::from_secs(
                        self.config.circuit_breaker_reset_timeout_seconds / 2 + 1,
                    );
                    tokio::time::sleep(wait_time).await;
                    continue;
                }
                CircuitState::HalfOpen => {
                    tracing::info!("Circuit breaker is half-open, attempting test connection");
                }
                CircuitState::Closed => {
                    // Normal operation
                }
            }

            self.health.set_reconnecting();

            match self.run_subscription_loop(&channels).await {
                Ok(()) => {
                    tracing::info!("Redis subscriber stopped gracefully");
                    break;
                }
                Err(e) => {
                    self.circuit_breaker.record_failure();

                    let delay = backoff.next_delay();
                    tracing::error!(
                        error = %e,
                        attempt = backoff.attempt(),
                        delay_ms = delay.as_millis(),
                        circuit_state = ?self.circuit_breaker.state(),
                        "Redis subscription error, reconnecting with backoff"
                    );

                    // Check for shutdown signal during wait
                    let mut shutdown_rx = self.shutdown.subscribe();
                    tokio::select! {
                        _ = shutdown_rx.recv() => {
                            tracing::info!("Shutdown requested during backoff");
                            break;
                        }
                        _ = tokio::time::sleep(delay) => {
                            // Continue to retry
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get configured channels
    fn get_channels(&self) -> Vec<String> {
        if self.config.channels.is_empty() {
            // Default channels if none configured
            vec![
                "notification:user:*".to_string(),
                "notification:broadcast".to_string(),
                "notification:channel:*".to_string(),
            ]
        } else {
            self.config.channels.clone()
        }
    }

    /// Run the subscription loop
    async fn run_subscription_loop(&self, channels: &[String]) -> anyhow::Result<()> {
        let client = redis::Client::open(self.config.url.as_str())?;
        let mut pubsub = client.get_async_pubsub().await?;

        // Subscribe to channels (with pattern support)
        for channel in channels {
            if channel.contains('*') || channel.contains('?') || channel.contains('[') {
                pubsub.psubscribe(channel).await?;
                tracing::debug!(pattern = %channel, "Subscribed to pattern");
            } else {
                pubsub.subscribe(channel).await?;
                tracing::debug!(channel = %channel, "Subscribed to channel");
            }
        }

        // Connection successful
        self.circuit_breaker.record_success();
        self.health.set_connected();
        tracing::info!("Redis subscription established");

        let mut message_stream = pubsub.on_message();
        let mut shutdown_rx = self.shutdown.subscribe();

        loop {
            tokio::select! {
                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    tracing::info!("Received shutdown signal");
                    break;
                }
                // Handle incoming messages
                msg = message_stream.next() => {
                    match msg {
                        Some(msg) => {
                            // Record successful message receipt
                            self.circuit_breaker.record_success();

                            let channel: String = msg.get_channel_name().to_string();
                            let payload: String = match msg.get_payload() {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to get message payload");
                                    continue;
                                }
                            };

                            self.handle_message(&channel, &payload).await;
                        }
                        None => {
                            tracing::warn!("Redis message stream ended");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle a received message
    async fn handle_message(&self, channel: &str, payload: &str) {
        tracing::debug!(channel = %channel, "Received Redis message");

        // Parse the message
        let message: RedisNotificationMessage = match serde_json::from_str(payload) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    channel = %channel,
                    payload = %payload,
                    "Failed to parse Redis message"
                );
                return;
            }
        };

        // Determine target first (before moving message fields)
        let target = match self.parse_target(&message) {
            Some(t) => t,
            None => {
                tracing::warn!(
                    target_type = %message.target_type,
                    "Unknown target type in Redis message"
                );
                return;
            }
        };

        // Build notification event
        let mut builder = NotificationBuilder::new(&message.event.event_type, format!("redis:{}", channel))
            .payload(message.event.payload)
            .priority(message.event.priority);

        if let Some(ttl) = message.event.ttl {
            builder = builder.ttl(ttl);
        }

        if let Some(correlation_id) = message.event.correlation_id {
            builder = builder.correlation_id(correlation_id);
        }

        let event = builder.build();

        let result = self.dispatcher.dispatch(target, event).await;

        tracing::debug!(
            channel = %channel,
            delivered = result.delivered_to,
            failed = result.failed,
            "Dispatched notification from Redis"
        );
    }

    /// Parse target from Redis message
    fn parse_target(&self, message: &RedisNotificationMessage) -> Option<NotificationTarget> {
        match message.target_type.as_str() {
            "user" => {
                let user_id = match &message.target {
                    Some(RedisTarget::Single(id)) => id.clone(),
                    _ => return None,
                };
                Some(NotificationTarget::User(user_id))
            }
            "users" => {
                let user_ids = match &message.target {
                    Some(RedisTarget::Multiple(ids)) => ids.clone(),
                    Some(RedisTarget::Single(id)) => vec![id.clone()],
                    None => return None,
                };
                Some(NotificationTarget::Users(user_ids))
            }
            "broadcast" => Some(NotificationTarget::Broadcast),
            "channel" => {
                let channel = match &message.target {
                    Some(RedisTarget::Single(ch)) => ch.clone(),
                    _ => return None,
                };
                Some(NotificationTarget::Channel(channel))
            }
            "channels" => {
                let channels = match &message.target {
                    Some(RedisTarget::Multiple(chs)) => chs.clone(),
                    Some(RedisTarget::Single(ch)) => vec![ch.clone()],
                    None => return None,
                };
                Some(NotificationTarget::Channels(channels))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user_message() {
        let json = r#"{
            "type": "user",
            "target": "user-123",
            "event": {
                "event_type": "order.created",
                "payload": {"order_id": "456"},
                "priority": "High"
            }
        }"#;

        let message: RedisNotificationMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.target_type, "user");
        assert_eq!(message.event.event_type, "order.created");
        assert_eq!(message.event.priority, Priority::High);
    }

    #[test]
    fn test_parse_broadcast_message() {
        let json = r#"{
            "type": "broadcast",
            "target": null,
            "event": {
                "event_type": "system.announcement",
                "payload": {"message": "Hello!"}
            }
        }"#;

        let message: RedisNotificationMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.target_type, "broadcast");
        assert!(message.target.is_none());
    }

    #[test]
    fn test_parse_channel_message() {
        let json = r#"{
            "type": "channel",
            "target": "orders",
            "event": {
                "event_type": "order.status_changed",
                "payload": {"order_id": "123", "status": "shipped"},
                "ttl": 3600,
                "correlation_id": "req-abc"
            }
        }"#;

        let message: RedisNotificationMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.target_type, "channel");
        assert_eq!(message.event.ttl, Some(3600));
        assert_eq!(message.event.correlation_id, Some("req-abc".to_string()));
    }

    #[test]
    fn test_parse_multiple_users_message() {
        let json = r#"{
            "type": "users",
            "target": ["user-1", "user-2", "user-3"],
            "event": {
                "event_type": "group.message",
                "payload": {"content": "Hello team!"}
            }
        }"#;

        let message: RedisNotificationMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.target_type, "users");
        match message.target {
            Some(RedisTarget::Multiple(users)) => {
                assert_eq!(users.len(), 3);
                assert_eq!(users[0], "user-1");
            }
            _ => panic!("Expected multiple targets"),
        }
    }

    #[test]
    fn test_parse_multiple_channels_message() {
        let json = r#"{
            "type": "channels",
            "target": ["orders", "inventory"],
            "event": {
                "event_type": "stock.update",
                "payload": {"product_id": "SKU-001"}
            }
        }"#;

        let message: RedisNotificationMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.target_type, "channels");
        match message.target {
            Some(RedisTarget::Multiple(channels)) => {
                assert_eq!(channels.len(), 2);
            }
            _ => panic!("Expected multiple targets"),
        }
    }
}

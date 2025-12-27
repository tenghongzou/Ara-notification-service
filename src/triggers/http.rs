use std::collections::HashSet;

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::notification::{Audience, NotificationBuilder, NotificationTarget, Priority};
use crate::server::AppState;
use crate::template::{substitute_variables, TemplateStore};

// ============================================================================
// Notification Content (Direct or Template-based)
// ============================================================================

/// Content specification for notifications - either direct or template-based
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum NotificationContent {
    /// Template-based content with variable substitution
    Template {
        /// Template ID to use
        template_id: String,
        /// Variables for template substitution
        #[serde(default = "default_empty_object")]
        variables: serde_json::Value,
    },
    /// Direct content specification
    Direct {
        /// Event type (e.g., "order.created")
        event_type: String,
        /// Event payload
        payload: serde_json::Value,
    },
}

fn default_empty_object() -> serde_json::Value {
    serde_json::json!({})
}

/// Resolved notification content ready for dispatch
struct ResolvedContent {
    event_type: String,
    payload: serde_json::Value,
    priority: Priority,
    ttl: Option<u32>,
}

impl NotificationContent {
    /// Resolve the content to event_type and payload
    fn resolve(
        self,
        template_store: &TemplateStore,
        priority_override: Option<Priority>,
        ttl_override: Option<u32>,
    ) -> Result<ResolvedContent> {
        match self {
            NotificationContent::Template {
                template_id,
                variables,
            } => {
                // Get the template
                let template = template_store
                    .get(&template_id)
                    .map_err(|e| AppError::Validation(e.to_string()))?;

                // Substitute variables in the payload template
                let payload = substitute_variables(&template.payload_template, &variables)
                    .map_err(|e| AppError::Validation(e.to_string()))?;

                Ok(ResolvedContent {
                    event_type: template.event_type,
                    payload,
                    priority: priority_override.unwrap_or(template.default_priority),
                    ttl: ttl_override.or(template.default_ttl),
                })
            }
            NotificationContent::Direct { event_type, payload } => Ok(ResolvedContent {
                event_type,
                payload,
                priority: priority_override.unwrap_or_default(),
                ttl: ttl_override,
            }),
        }
    }
}

/// Request to send notification to a specific user
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct SendNotificationRequest {
    /// Target user ID
    pub target_user_id: String,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Request to send notification to multiple users
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct SendToUsersRequest {
    /// Target user IDs
    pub target_user_ids: Vec<String>,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to broadcast notification to all users
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct BroadcastNotificationRequest {
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional target audience filter
    pub audience: Option<Audience>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to send notification to a channel
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct ChannelNotificationRequest {
    /// Target channel name
    pub channel: String,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Request to send notification to multiple channels
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct MultiChannelNotificationRequest {
    /// Target channel names
    pub channels: Vec<String>,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Response for notification send operations
#[derive(Debug, Serialize)]
pub struct SendNotificationResponse {
    /// Whether the operation was successful
    pub success: bool,
    /// Notification ID
    pub notification_id: Uuid,
    /// Number of connections the notification was delivered to
    pub delivered_to: usize,
    /// Number of failed deliveries
    pub failed: usize,
    /// Timestamp of the operation
    pub timestamp: DateTime<Utc>,
}

const SOURCE: &str = "http-api";

/// Send notification to a specific user
#[tracing::instrument(
    name = "http.send_notification",
    skip(state, request),
    fields(target_user_id = %request.target_user_id)
)]
pub async fn send_notification(
    State(state): State<AppState>,
    Json(request): Json<SendNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state
        .dispatcher
        .send_to_user(&request.target_user_id, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

/// Send notification to multiple users
#[tracing::instrument(
    name = "http.send_to_users",
    skip(state, request),
    fields(user_count = request.target_user_ids.len())
)]
pub async fn send_to_users(
    State(state): State<AppState>,
    Json(request): Json<SendToUsersRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state
        .dispatcher
        .send_to_users(&request.target_user_ids, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

/// Broadcast notification to all connected users
#[tracing::instrument(
    name = "http.broadcast",
    skip(state, request),
    fields(audience = ?request.audience)
)]
pub async fn broadcast_notification(
    State(state): State<AppState>,
    Json(request): Json<BroadcastNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(audience) = request.audience {
        builder = builder.audience(audience);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state.dispatcher.broadcast(event).await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

/// Send notification to a channel
#[tracing::instrument(
    name = "http.channel_notification",
    skip(state, request),
    fields(channel = %request.channel)
)]
pub async fn channel_notification(
    State(state): State<AppState>,
    Json(request): Json<ChannelNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state
        .dispatcher
        .send_to_channel(&request.channel, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

/// Send notification to multiple channels
#[tracing::instrument(
    name = "http.multi_channel_notification",
    skip(state, request),
    fields(channel_count = request.channels.len())
)]
pub async fn multi_channel_notification(
    State(state): State<AppState>,
    Json(request): Json<MultiChannelNotificationRequest>,
) -> Result<Json<SendNotificationResponse>> {
    // Resolve content (from template or direct)
    let resolved = request
        .content
        .resolve(&state.template_store, request.priority, request.ttl)?;

    let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
        .payload(resolved.payload)
        .priority(resolved.priority);

    if let Some(ttl) = resolved.ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(correlation_id) = request.correlation_id {
        builder = builder.correlation_id(correlation_id);
    }

    let event = builder.build();
    let result = state
        .dispatcher
        .send_to_channels(&request.channels, event)
        .await;

    Ok(Json(SendNotificationResponse {
        success: result.success,
        notification_id: result.notification_id,
        delivered_to: result.delivered_to,
        failed: result.failed,
        timestamp: Utc::now(),
    }))
}

// ============================================================================
// Batch Send API
// ============================================================================

/// Maximum number of notifications per batch
const MAX_BATCH_SIZE: usize = 100;

/// Target specification for batch notifications
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum BatchTarget {
    /// Send to a specific user
    #[serde(rename = "user")]
    User(String),
    /// Send to multiple users
    #[serde(rename = "users")]
    Users(Vec<String>),
    /// Broadcast to all connected users
    #[serde(rename = "broadcast")]
    Broadcast,
    /// Send to users subscribed to a channel
    #[serde(rename = "channel")]
    Channel(String),
    /// Send to users subscribed to multiple channels
    #[serde(rename = "channels")]
    Channels(Vec<String>),
}

impl BatchTarget {
    /// Convert to NotificationTarget
    fn into_notification_target(self) -> NotificationTarget {
        match self {
            BatchTarget::User(id) => NotificationTarget::User(id),
            BatchTarget::Users(ids) => NotificationTarget::Users(ids),
            BatchTarget::Broadcast => NotificationTarget::Broadcast,
            BatchTarget::Channel(name) => NotificationTarget::Channel(name),
            BatchTarget::Channels(names) => NotificationTarget::Channels(names),
        }
    }

    /// Get a string key for deduplication
    fn dedup_key(&self) -> String {
        match self {
            BatchTarget::User(id) => format!("user:{}", id),
            BatchTarget::Users(ids) => {
                let mut sorted = ids.clone();
                sorted.sort();
                format!("users:{}", sorted.join(","))
            }
            BatchTarget::Broadcast => "broadcast".to_string(),
            BatchTarget::Channel(name) => format!("channel:{}", name),
            BatchTarget::Channels(names) => {
                let mut sorted = names.clone();
                sorted.sort();
                format!("channels:{}", sorted.join(","))
            }
        }
    }
}

/// Single notification item in a batch request
///
/// Supports two content modes:
/// 1. Direct: `{ "event_type": "...", "payload": {...} }`
/// 2. Template: `{ "template_id": "...", "variables": {...} }`
#[derive(Debug, Deserialize)]
pub struct BatchNotificationItem {
    /// Target for this notification
    pub target: BatchTarget,
    /// Notification content (direct or template-based)
    #[serde(flatten)]
    pub content: NotificationContent,
    /// Priority level (overrides template default if provided)
    pub priority: Option<Priority>,
    /// Optional TTL in seconds (overrides template default if provided)
    pub ttl: Option<u32>,
    /// Optional correlation ID
    pub correlation_id: Option<String>,
}

/// Options for batch send
#[derive(Debug, Deserialize, Default)]
pub struct BatchOptions {
    /// Stop processing on first error
    #[serde(default)]
    pub stop_on_error: bool,
    /// Skip duplicate targets (based on target+event_type)
    #[serde(default)]
    pub deduplicate: bool,
}

/// Batch send request
#[derive(Debug, Deserialize)]
pub struct BatchSendRequest {
    /// List of notifications to send
    pub notifications: Vec<BatchNotificationItem>,
    /// Batch options
    #[serde(default)]
    pub options: BatchOptions,
}

/// Result for a single notification in batch
#[derive(Debug, Serialize)]
pub struct BatchItemResult {
    /// Index in the original request
    pub index: usize,
    /// Notification ID
    pub notification_id: Uuid,
    /// Number of connections the notification was delivered to
    pub delivered_to: usize,
    /// Number of failed deliveries
    pub failed: usize,
    /// Whether this notification was successful
    pub success: bool,
    /// Error message if failed (only present on error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Whether this item was skipped due to deduplication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<bool>,
}

/// Summary of batch send operation
#[derive(Debug, Serialize)]
pub struct BatchSummary {
    /// Total notifications in request
    pub total: usize,
    /// Number of successful notifications
    pub succeeded: usize,
    /// Number of failed notifications
    pub failed: usize,
    /// Number of skipped notifications (deduplication)
    pub skipped: usize,
    /// Total connections delivered to
    pub total_delivered: usize,
}

/// Response for batch send operation
#[derive(Debug, Serialize)]
pub struct BatchSendResponse {
    /// Unique batch ID
    pub batch_id: Uuid,
    /// Results for each notification
    pub results: Vec<BatchItemResult>,
    /// Summary of the batch operation
    pub summary: BatchSummary,
    /// Timestamp of the operation
    pub timestamp: DateTime<Utc>,
}

/// Send notifications in batch
///
/// Supports up to 100 notifications per batch with optional deduplication
/// and stop-on-error behavior.
#[tracing::instrument(
    name = "http.batch_send",
    skip(state, request),
    fields(
        batch_size = request.notifications.len(),
        stop_on_error = request.options.stop_on_error,
        deduplicate = request.options.deduplicate
    )
)]
pub async fn batch_send(
    State(state): State<AppState>,
    Json(request): Json<BatchSendRequest>,
) -> Result<Json<BatchSendResponse>> {
    let batch_id = Uuid::new_v4();
    let total = request.notifications.len();

    // Validate batch size
    if total > MAX_BATCH_SIZE {
        return Err(AppError::Validation(format!(
            "Batch size {} exceeds maximum of {}",
            total, MAX_BATCH_SIZE
        )));
    }

    if total == 0 {
        return Err(AppError::Validation("Batch cannot be empty".to_string()));
    }

    let mut results = Vec::with_capacity(total);
    let mut succeeded = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut total_delivered = 0;

    // For deduplication
    let mut seen_keys: HashSet<String> = HashSet::new();

    for (index, item) in request.notifications.into_iter().enumerate() {
        // Resolve content (from template or direct)
        let resolved = match item
            .content
            .resolve(&state.template_store, item.priority, item.ttl)
        {
            Ok(r) => r,
            Err(e) => {
                results.push(BatchItemResult {
                    index,
                    notification_id: Uuid::nil(),
                    delivered_to: 0,
                    failed: 0,
                    success: false,
                    error: Some(e.to_string()),
                    skipped: None,
                });
                failed += 1;
                if request.options.stop_on_error {
                    break;
                }
                continue;
            }
        };

        // Check for deduplication
        if request.options.deduplicate {
            let dedup_key = format!("{}:{}", item.target.dedup_key(), resolved.event_type);
            if seen_keys.contains(&dedup_key) {
                results.push(BatchItemResult {
                    index,
                    notification_id: Uuid::nil(),
                    delivered_to: 0,
                    failed: 0,
                    success: true,
                    error: None,
                    skipped: Some(true),
                });
                skipped += 1;
                continue;
            }
            seen_keys.insert(dedup_key);
        }

        // Build notification event
        let mut builder = NotificationBuilder::new(&resolved.event_type, SOURCE)
            .payload(resolved.payload)
            .priority(resolved.priority);

        if let Some(ttl) = resolved.ttl {
            builder = builder.ttl(ttl);
        }

        if let Some(correlation_id) = item.correlation_id {
            builder = builder.correlation_id(correlation_id);
        }

        let event = builder.build();
        let target = item.target.into_notification_target();

        // Dispatch notification
        let result = state.dispatcher.dispatch(target, event).await;

        let item_success = result.success;
        total_delivered += result.delivered_to;

        results.push(BatchItemResult {
            index,
            notification_id: result.notification_id,
            delivered_to: result.delivered_to,
            failed: result.failed,
            success: item_success,
            error: None,
            skipped: None,
        });

        if item_success {
            succeeded += 1;
        } else {
            failed += 1;

            // Stop on first error if requested
            if request.options.stop_on_error {
                // Mark remaining items as skipped
                for remaining_index in (index + 1)..total {
                    results.push(BatchItemResult {
                        index: remaining_index,
                        notification_id: Uuid::nil(),
                        delivered_to: 0,
                        failed: 0,
                        success: false,
                        error: Some("Skipped due to stop_on_error".to_string()),
                        skipped: Some(true),
                    });
                    skipped += 1;
                }
                break;
            }
        }
    }

    Ok(Json(BatchSendResponse {
        batch_id,
        results,
        summary: BatchSummary {
            total,
            succeeded,
            failed,
            skipped,
            total_delivered,
        },
        timestamp: Utc::now(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_target_user_dedup_key() {
        let target = BatchTarget::User("user-123".to_string());
        assert_eq!(target.dedup_key(), "user:user-123");
    }

    #[test]
    fn test_batch_target_users_dedup_key() {
        let target = BatchTarget::Users(vec!["user-3".to_string(), "user-1".to_string(), "user-2".to_string()]);
        // Should be sorted
        assert_eq!(target.dedup_key(), "users:user-1,user-2,user-3");
    }

    #[test]
    fn test_batch_target_broadcast_dedup_key() {
        let target = BatchTarget::Broadcast;
        assert_eq!(target.dedup_key(), "broadcast");
    }

    #[test]
    fn test_batch_target_channel_dedup_key() {
        let target = BatchTarget::Channel("orders".to_string());
        assert_eq!(target.dedup_key(), "channel:orders");
    }

    #[test]
    fn test_batch_target_channels_dedup_key() {
        let target = BatchTarget::Channels(vec!["orders".to_string(), "alerts".to_string()]);
        // Should be sorted
        assert_eq!(target.dedup_key(), "channels:alerts,orders");
    }

    #[test]
    fn test_batch_target_into_notification_target() {
        let target = BatchTarget::User("user-123".to_string());
        let notification_target = target.into_notification_target();
        match notification_target {
            NotificationTarget::User(id) => assert_eq!(id, "user-123"),
            _ => panic!("Expected User target"),
        }

        let target = BatchTarget::Broadcast;
        let notification_target = target.into_notification_target();
        match notification_target {
            NotificationTarget::Broadcast => {}
            _ => panic!("Expected Broadcast target"),
        }
    }

    #[test]
    fn test_batch_target_parse_user() {
        let json = r#"{"type": "user", "value": "user-123"}"#;
        let target: BatchTarget = serde_json::from_str(json).unwrap();
        match target {
            BatchTarget::User(id) => assert_eq!(id, "user-123"),
            _ => panic!("Expected User target"),
        }
    }

    #[test]
    fn test_batch_target_parse_broadcast() {
        let json = r#"{"type": "broadcast"}"#;
        let target: BatchTarget = serde_json::from_str(json).unwrap();
        match target {
            BatchTarget::Broadcast => {}
            _ => panic!("Expected Broadcast target"),
        }
    }

    #[test]
    fn test_batch_target_parse_channel() {
        let json = r#"{"type": "channel", "value": "orders"}"#;
        let target: BatchTarget = serde_json::from_str(json).unwrap();
        match target {
            BatchTarget::Channel(name) => assert_eq!(name, "orders"),
            _ => panic!("Expected Channel target"),
        }
    }

    #[test]
    fn test_batch_target_parse_users() {
        let json = r#"{"type": "users", "value": ["user-1", "user-2"]}"#;
        let target: BatchTarget = serde_json::from_str(json).unwrap();
        match target {
            BatchTarget::Users(ids) => {
                assert_eq!(ids.len(), 2);
                assert_eq!(ids[0], "user-1");
            }
            _ => panic!("Expected Users target"),
        }
    }

    #[test]
    fn test_batch_notification_item_parse() {
        let json = r#"{
            "target": {"type": "user", "value": "user-123"},
            "event_type": "order.created",
            "payload": {"order_id": "456"},
            "priority": "High",
            "ttl": 3600
        }"#;

        let item: BatchNotificationItem = serde_json::from_str(json).unwrap();
        match item.content {
            NotificationContent::Direct { event_type, .. } => {
                assert_eq!(event_type, "order.created");
            }
            _ => panic!("Expected Direct content"),
        }
        assert_eq!(item.priority, Some(Priority::High));
        assert_eq!(item.ttl, Some(3600));
    }

    #[test]
    fn test_batch_notification_item_parse_template() {
        let json = r#"{
            "target": {"type": "user", "value": "user-123"},
            "template_id": "order-shipped",
            "variables": {"order_id": "456", "carrier": "FedEx"}
        }"#;

        let item: BatchNotificationItem = serde_json::from_str(json).unwrap();
        match item.content {
            NotificationContent::Template { template_id, variables } => {
                assert_eq!(template_id, "order-shipped");
                assert_eq!(variables["order_id"], "456");
                assert_eq!(variables["carrier"], "FedEx");
            }
            _ => panic!("Expected Template content"),
        }
    }

    #[test]
    fn test_batch_request_parse() {
        let json = r#"{
            "notifications": [
                {
                    "target": {"type": "user", "value": "user-1"},
                    "event_type": "order.created",
                    "payload": {}
                },
                {
                    "target": {"type": "broadcast"},
                    "event_type": "system.maintenance",
                    "payload": {"message": "Maintenance in 1 hour"}
                }
            ],
            "options": {
                "stop_on_error": true,
                "deduplicate": true
            }
        }"#;

        let request: BatchSendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.notifications.len(), 2);
        assert!(request.options.stop_on_error);
        assert!(request.options.deduplicate);
    }

    #[test]
    fn test_batch_options_default() {
        let json = r#"{
            "notifications": []
        }"#;

        let request: BatchSendRequest = serde_json::from_str(json).unwrap();
        assert!(!request.options.stop_on_error);
        assert!(!request.options.deduplicate);
    }
}

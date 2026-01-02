use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Notification event that gets sent to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    /// Unique identifier for this notification
    pub id: Uuid,
    /// When the event occurred
    pub occurred_at: DateTime<Utc>,
    /// Type of event (e.g., "order.created", "user.updated")
    pub event_type: String,
    /// Event payload data
    pub payload: serde_json::Value,
    /// Event metadata
    pub metadata: NotificationMetadata,
}

/// Metadata associated with a notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMetadata {
    /// Source service that generated this notification
    pub source: String,
    /// Priority level
    pub priority: Priority,
    /// Time-to-live in seconds (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u32>,
    /// Target audience filter (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Audience>,
    /// Correlation ID for tracing (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

/// Priority levels for notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum Priority {
    /// Low priority, can be delayed
    Low,
    /// Normal priority (default)
    #[default]
    Normal,
    /// High priority, should be delivered promptly
    High,
    /// Critical priority, immediate delivery required
    Critical,
}

/// Target audience for notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Audience {
    /// All users
    All,
    /// Users with specific roles
    Roles(Vec<String>),
    /// Specific user IDs
    Users(Vec<String>),
    /// Users subscribed to specific channels
    Channels(Vec<String>),
}

/// Notification target specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "target")]
pub enum NotificationTarget {
    /// Send to a specific user
    User(String),
    /// Send to multiple users
    Users(Vec<String>),
    /// Broadcast to all connected users
    Broadcast,
    /// Send to users subscribed to a channel
    Channel(String),
    /// Send to users subscribed to multiple channels
    Channels(Vec<String>),
}

/// Builder for creating notification events
#[derive(Debug, Clone)]
pub struct NotificationBuilder {
    event_type: String,
    payload: serde_json::Value,
    source: String,
    priority: Priority,
    ttl: Option<u32>,
    audience: Option<Audience>,
    correlation_id: Option<String>,
}

impl NotificationBuilder {
    /// Create a new notification builder
    pub fn new(event_type: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            event_type: event_type.into(),
            payload: serde_json::Value::Null,
            source: source.into(),
            priority: Priority::default(),
            ttl: None,
            audience: None,
            correlation_id: None,
        }
    }

    /// Set the payload
    pub fn payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    /// Set the payload from a serializable value
    pub fn payload_from<T: Serialize>(mut self, payload: &T) -> Result<Self, serde_json::Error> {
        self.payload = serde_json::to_value(payload)?;
        Ok(self)
    }

    /// Set the priority
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set time-to-live in seconds
    pub fn ttl(mut self, ttl: u32) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Set the target audience
    pub fn audience(mut self, audience: Audience) -> Self {
        self.audience = Some(audience);
        self
    }

    /// Set the correlation ID for tracing
    pub fn correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Build the notification event
    pub fn build(self) -> NotificationEvent {
        NotificationEvent {
            id: Uuid::new_v4(),
            occurred_at: Utc::now(),
            event_type: self.event_type,
            payload: self.payload,
            metadata: NotificationMetadata {
                source: self.source,
                priority: self.priority,
                ttl: self.ttl,
                audience: self.audience,
                correlation_id: self.correlation_id,
            },
        }
    }
}

impl NotificationEvent {
    /// Create a new notification event with minimal parameters
    pub fn new(
        event_type: impl Into<String>,
        payload: serde_json::Value,
        source: impl Into<String>,
    ) -> Self {
        NotificationBuilder::new(event_type, source)
            .payload(payload)
            .build()
    }

    /// Create a builder for this notification type
    pub fn builder(event_type: impl Into<String>, source: impl Into<String>) -> NotificationBuilder {
        NotificationBuilder::new(event_type, source)
    }

    /// Check if the notification has expired based on TTL
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.metadata.ttl {
            let expiry = self.occurred_at + chrono::Duration::seconds(ttl as i64);
            Utc::now() > expiry
        } else {
            false
        }
    }

    /// Check if notification should be delivered to a user with given roles
    pub fn should_deliver_to_roles(&self, user_roles: &[String]) -> bool {
        match &self.metadata.audience {
            None => true,
            Some(Audience::All) => true,
            Some(Audience::Roles(required_roles)) => {
                required_roles.iter().any(|r| user_roles.contains(r))
            }
            Some(Audience::Users(_)) => true, // User filtering is handled separately
            Some(Audience::Channels(_)) => true, // Channel filtering is handled separately
        }
    }
}

impl NotificationMetadata {
    /// Create new metadata with just a source
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            priority: Priority::default(),
            ttl: None,
            audience: None,
            correlation_id: None,
        }
    }
}

impl Priority {
    /// Get numeric value for priority comparison
    pub fn as_weight(&self) -> u8 {
        match self {
            Priority::Low => 1,
            Priority::Normal => 2,
            Priority::High => 3,
            Priority::Critical => 4,
        }
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_weight().cmp(&other.as_weight())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_builder() {
        let event = NotificationBuilder::new("order.created", "test-service")
            .payload(serde_json::json!({"order_id": "123"}))
            .priority(Priority::High)
            .ttl(3600)
            .correlation_id("req-456")
            .build();

        assert_eq!(event.event_type, "order.created");
        assert_eq!(event.metadata.source, "test-service");
        assert_eq!(event.metadata.priority, Priority::High);
        assert_eq!(event.metadata.ttl, Some(3600));
        assert_eq!(event.metadata.correlation_id, Some("req-456".to_string()));
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }

    #[test]
    fn test_expired_notification() {
        let mut event = NotificationEvent::new(
            "test",
            serde_json::Value::Null,
            "test",
        );

        // No TTL - never expires
        assert!(!event.is_expired());

        // Set TTL to 0 and backdate
        event.metadata.ttl = Some(0);
        event.occurred_at = Utc::now() - chrono::Duration::seconds(1);
        assert!(event.is_expired());
    }

    #[test]
    fn test_audience_role_filtering() {
        let event = NotificationBuilder::new("test", "test")
            .audience(Audience::Roles(vec!["admin".to_string()]))
            .build();

        assert!(event.should_deliver_to_roles(&["admin".to_string()]));
        assert!(event.should_deliver_to_roles(&["user".to_string(), "admin".to_string()]));
        assert!(!event.should_deliver_to_roles(&["user".to_string()]));
    }
}

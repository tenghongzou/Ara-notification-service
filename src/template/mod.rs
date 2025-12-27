//! Notification template system.
//!
//! This module provides:
//! - Template definition with variable placeholders ({{variable}})
//! - In-memory template storage with CRUD operations
//! - Variable substitution engine for rendering templates
//!
//! # Example
//!
//! ```ignore
//! let store = TemplateStore::new();
//!
//! // Create a template
//! let template = Template {
//!     id: "order-shipped".to_string(),
//!     name: "Order Shipped".to_string(),
//!     event_type: "order.shipped".to_string(),
//!     payload_template: json!({
//!         "title": "Your order has shipped",
//!         "body": "Order {{order_id}} is on its way via {{carrier}}"
//!     }),
//!     default_priority: Priority::High,
//!     default_ttl: Some(86400),
//! };
//!
//! store.create(template)?;
//!
//! // Render with variables
//! let variables = json!({
//!     "order_id": "ORD-123",
//!     "carrier": "FedEx"
//! });
//!
//! let rendered = store.render("order-shipped", &variables)?;
//! ```

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::notification::Priority;

/// Template-specific error type
#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    NotFound(String),

    #[error("Template already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid template ID: {0}")]
    InvalidId(String),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Variable substitution failed: {0}")]
    SubstitutionFailed(String),
}

/// Result type for template operations
pub type TemplateResult<T> = Result<T, TemplateError>;

/// A notification template definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Unique template identifier (alphanumeric, dash, underscore)
    pub id: String,

    /// Human-readable template name
    pub name: String,

    /// Event type to use when sending notifications
    pub event_type: String,

    /// JSON payload template with {{variable}} placeholders
    pub payload_template: serde_json::Value,

    /// Default priority for notifications using this template
    #[serde(default)]
    pub default_priority: Priority,

    /// Default TTL in seconds (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_ttl: Option<u32>,

    /// Template description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

impl Template {
    /// Validate the template
    pub fn validate(&self) -> TemplateResult<()> {
        // Validate ID
        if self.id.is_empty() || self.id.len() > 64 {
            return Err(TemplateError::InvalidId(
                "ID must be 1-64 characters".to_string(),
            ));
        }

        if !self.id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(TemplateError::InvalidId(
                "ID must contain only alphanumeric, dash, or underscore".to_string(),
            ));
        }

        // Validate name
        if self.name.is_empty() || self.name.len() > 256 {
            return Err(TemplateError::InvalidTemplate(
                "Name must be 1-256 characters".to_string(),
            ));
        }

        // Validate event_type
        if self.event_type.is_empty() || self.event_type.len() > 128 {
            return Err(TemplateError::InvalidTemplate(
                "Event type must be 1-128 characters".to_string(),
            ));
        }

        Ok(())
    }
}

/// Request to create a new template
#[derive(Debug, Deserialize)]
pub struct CreateTemplateRequest {
    /// Unique template identifier
    pub id: String,

    /// Human-readable template name
    pub name: String,

    /// Event type to use
    pub event_type: String,

    /// JSON payload template with {{variable}} placeholders
    pub payload_template: serde_json::Value,

    /// Default priority (optional, defaults to Normal)
    #[serde(default)]
    pub default_priority: Priority,

    /// Default TTL in seconds (optional)
    pub default_ttl: Option<u32>,

    /// Template description (optional)
    pub description: Option<String>,
}

impl From<CreateTemplateRequest> for Template {
    fn from(req: CreateTemplateRequest) -> Self {
        let now = Utc::now();
        Template {
            id: req.id,
            name: req.name,
            event_type: req.event_type,
            payload_template: req.payload_template,
            default_priority: req.default_priority,
            default_ttl: req.default_ttl,
            description: req.description,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Request to update an existing template
#[derive(Debug, Deserialize)]
pub struct UpdateTemplateRequest {
    /// Human-readable template name (optional)
    pub name: Option<String>,

    /// Event type to use (optional)
    pub event_type: Option<String>,

    /// JSON payload template (optional)
    pub payload_template: Option<serde_json::Value>,

    /// Default priority (optional)
    pub default_priority: Option<Priority>,

    /// Default TTL in seconds (optional, use null to clear)
    pub default_ttl: Option<Option<u32>>,

    /// Template description (optional, use null to clear)
    pub description: Option<Option<String>>,
}

/// Response for listing templates
#[derive(Debug, Serialize)]
pub struct TemplateListResponse {
    /// List of templates
    pub templates: Vec<Template>,

    /// Total count
    pub total: usize,
}

/// In-memory template storage
pub struct TemplateStore {
    templates: DashMap<String, Template>,
}

impl Default for TemplateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateStore {
    /// Create a new template store
    pub fn new() -> Self {
        Self {
            templates: DashMap::new(),
        }
    }

    /// Create a new template
    pub fn create(&self, template: Template) -> TemplateResult<Template> {
        template.validate()?;

        if self.templates.contains_key(&template.id) {
            return Err(TemplateError::AlreadyExists(template.id));
        }

        let id = template.id.clone();
        self.templates.insert(id.clone(), template);

        Ok(self.templates.get(&id).unwrap().clone())
    }

    /// Get a template by ID
    pub fn get(&self, id: &str) -> TemplateResult<Template> {
        self.templates
            .get(id)
            .map(|t| t.clone())
            .ok_or_else(|| TemplateError::NotFound(id.to_string()))
    }

    /// List all templates
    pub fn list(&self) -> Vec<Template> {
        self.templates
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Update an existing template
    pub fn update(&self, id: &str, updates: UpdateTemplateRequest) -> TemplateResult<Template> {
        let mut template = self.get(id)?;

        if let Some(name) = updates.name {
            template.name = name;
        }

        if let Some(event_type) = updates.event_type {
            template.event_type = event_type;
        }

        if let Some(payload_template) = updates.payload_template {
            template.payload_template = payload_template;
        }

        if let Some(priority) = updates.default_priority {
            template.default_priority = priority;
        }

        if let Some(ttl) = updates.default_ttl {
            template.default_ttl = ttl;
        }

        if let Some(description) = updates.description {
            template.description = description;
        }

        template.updated_at = Utc::now();
        template.validate()?;

        self.templates.insert(id.to_string(), template.clone());

        Ok(template)
    }

    /// Delete a template by ID
    pub fn delete(&self, id: &str) -> TemplateResult<()> {
        self.templates
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| TemplateError::NotFound(id.to_string()))
    }

    /// Check if a template exists
    pub fn exists(&self, id: &str) -> bool {
        self.templates.contains_key(id)
    }

    /// Get the number of templates
    pub fn count(&self) -> usize {
        self.templates.len()
    }

    /// Render a template with variables
    pub fn render(
        &self,
        id: &str,
        variables: &serde_json::Value,
    ) -> TemplateResult<RenderedTemplate> {
        let template = self.get(id)?;

        let rendered_payload = substitute_variables(&template.payload_template, variables)?;

        Ok(RenderedTemplate {
            event_type: template.event_type,
            payload: rendered_payload,
            priority: template.default_priority,
            ttl: template.default_ttl,
        })
    }
}

/// A rendered template ready for notification creation
#[derive(Debug, Clone)]
pub struct RenderedTemplate {
    /// Event type
    pub event_type: String,

    /// Rendered payload
    pub payload: serde_json::Value,

    /// Priority from template
    pub priority: Priority,

    /// TTL from template
    pub ttl: Option<u32>,
}

/// Substitute {{variable}} placeholders in a JSON value
pub fn substitute_variables(
    template: &serde_json::Value,
    variables: &serde_json::Value,
) -> TemplateResult<serde_json::Value> {
    let vars = match variables {
        serde_json::Value::Object(map) => map,
        _ => {
            return Err(TemplateError::SubstitutionFailed(
                "Variables must be an object".to_string(),
            ))
        }
    };

    substitute_value(template, vars)
}

fn substitute_value(
    value: &serde_json::Value,
    variables: &serde_json::Map<String, serde_json::Value>,
) -> TemplateResult<serde_json::Value> {
    match value {
        serde_json::Value::String(s) => Ok(serde_json::Value::String(substitute_string(s, variables))),
        serde_json::Value::Array(arr) => {
            let rendered: Result<Vec<_>, _> = arr
                .iter()
                .map(|v| substitute_value(v, variables))
                .collect();
            Ok(serde_json::Value::Array(rendered?))
        }
        serde_json::Value::Object(obj) => {
            let mut rendered = serde_json::Map::new();
            for (key, val) in obj {
                let rendered_key = substitute_string(key, variables);
                let rendered_val = substitute_value(val, variables)?;
                rendered.insert(rendered_key, rendered_val);
            }
            Ok(serde_json::Value::Object(rendered))
        }
        // Numbers, booleans, null are passed through as-is
        _ => Ok(value.clone()),
    }
}

fn substitute_string(
    template: &str,
    variables: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let mut result = template.to_string();

    // Find all {{variable}} patterns and replace them
    for (key, value) in variables {
        let pattern = format!("{{{{{}}}}}", key);
        let replacement = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => "".to_string(),
            // For arrays and objects, use JSON representation
            _ => value.to_string(),
        };
        result = result.replace(&pattern, &replacement);
    }

    result
}

/// Create an Arc-wrapped template store
pub fn create_template_store() -> Arc<TemplateStore> {
    Arc::new(TemplateStore::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_template_validation_valid() {
        let template = Template {
            id: "order-shipped".to_string(),
            name: "Order Shipped".to_string(),
            event_type: "order.shipped".to_string(),
            payload_template: json!({"message": "Hello"}),
            default_priority: Priority::Normal,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(template.validate().is_ok());
    }

    #[test]
    fn test_template_validation_empty_id() {
        let template = Template {
            id: "".to_string(),
            name: "Test".to_string(),
            event_type: "test".to_string(),
            payload_template: json!({}),
            default_priority: Priority::Normal,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(matches!(
            template.validate(),
            Err(TemplateError::InvalidId(_))
        ));
    }

    #[test]
    fn test_template_validation_invalid_id_chars() {
        let template = Template {
            id: "invalid/id".to_string(),
            name: "Test".to_string(),
            event_type: "test".to_string(),
            payload_template: json!({}),
            default_priority: Priority::Normal,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(matches!(
            template.validate(),
            Err(TemplateError::InvalidId(_))
        ));
    }

    #[test]
    fn test_store_create_and_get() {
        let store = TemplateStore::new();

        let template = Template {
            id: "test-template".to_string(),
            name: "Test Template".to_string(),
            event_type: "test.event".to_string(),
            payload_template: json!({"key": "value"}),
            default_priority: Priority::High,
            default_ttl: Some(3600),
            description: Some("A test template".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let created = store.create(template).unwrap();
        assert_eq!(created.id, "test-template");

        let retrieved = store.get("test-template").unwrap();
        assert_eq!(retrieved.name, "Test Template");
    }

    #[test]
    fn test_store_create_duplicate() {
        let store = TemplateStore::new();

        let template = Template {
            id: "duplicate".to_string(),
            name: "Test".to_string(),
            event_type: "test".to_string(),
            payload_template: json!({}),
            default_priority: Priority::Normal,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store.create(template.clone()).unwrap();
        assert!(matches!(
            store.create(template),
            Err(TemplateError::AlreadyExists(_))
        ));
    }

    #[test]
    fn test_store_update() {
        let store = TemplateStore::new();

        let template = Template {
            id: "update-test".to_string(),
            name: "Original".to_string(),
            event_type: "test".to_string(),
            payload_template: json!({}),
            default_priority: Priority::Normal,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store.create(template).unwrap();

        let updates = UpdateTemplateRequest {
            name: Some("Updated".to_string()),
            event_type: None,
            payload_template: None,
            default_priority: Some(Priority::High),
            default_ttl: None,
            description: None,
        };

        let updated = store.update("update-test", updates).unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.default_priority, Priority::High);
    }

    #[test]
    fn test_store_delete() {
        let store = TemplateStore::new();

        let template = Template {
            id: "delete-test".to_string(),
            name: "Test".to_string(),
            event_type: "test".to_string(),
            payload_template: json!({}),
            default_priority: Priority::Normal,
            default_ttl: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store.create(template).unwrap();
        assert!(store.exists("delete-test"));

        store.delete("delete-test").unwrap();
        assert!(!store.exists("delete-test"));
    }

    #[test]
    fn test_store_list() {
        let store = TemplateStore::new();

        for i in 0..3 {
            let template = Template {
                id: format!("template-{}", i),
                name: format!("Template {}", i),
                event_type: "test".to_string(),
                payload_template: json!({}),
                default_priority: Priority::Normal,
                default_ttl: None,
                description: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            store.create(template).unwrap();
        }

        let list = store.list();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_substitute_simple() {
        let template = json!({
            "message": "Hello, {{name}}!"
        });

        let variables = json!({
            "name": "World"
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["message"], "Hello, World!");
    }

    #[test]
    fn test_substitute_multiple() {
        let template = json!({
            "title": "Order {{order_id}} shipped",
            "body": "Your order {{order_id}} is being delivered by {{carrier}}"
        });

        let variables = json!({
            "order_id": "ORD-123",
            "carrier": "FedEx"
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["title"], "Order ORD-123 shipped");
        assert_eq!(
            result["body"],
            "Your order ORD-123 is being delivered by FedEx"
        );
    }

    #[test]
    fn test_substitute_nested() {
        let template = json!({
            "notification": {
                "title": "Hello {{name}}",
                "data": {
                    "user_id": "{{user_id}}"
                }
            }
        });

        let variables = json!({
            "name": "Alice",
            "user_id": "user-123"
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["notification"]["title"], "Hello Alice");
        assert_eq!(result["notification"]["data"]["user_id"], "user-123");
    }

    #[test]
    fn test_substitute_array() {
        let template = json!({
            "items": ["{{item1}}", "{{item2}}"]
        });

        let variables = json!({
            "item1": "First",
            "item2": "Second"
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["items"][0], "First");
        assert_eq!(result["items"][1], "Second");
    }

    #[test]
    fn test_substitute_number_variable() {
        let template = json!({
            "count": "You have {{count}} items"
        });

        let variables = json!({
            "count": 42
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["count"], "You have 42 items");
    }

    #[test]
    fn test_render_template() {
        let store = TemplateStore::new();

        let template = Template {
            id: "order-shipped".to_string(),
            name: "Order Shipped".to_string(),
            event_type: "order.shipped".to_string(),
            payload_template: json!({
                "title": "Order {{order_id}} shipped",
                "tracking": "{{tracking_number}}"
            }),
            default_priority: Priority::High,
            default_ttl: Some(86400),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store.create(template).unwrap();

        let variables = json!({
            "order_id": "ORD-456",
            "tracking_number": "TW123456789"
        });

        let rendered = store.render("order-shipped", &variables).unwrap();
        assert_eq!(rendered.event_type, "order.shipped");
        assert_eq!(rendered.payload["title"], "Order ORD-456 shipped");
        assert_eq!(rendered.payload["tracking"], "TW123456789");
        assert_eq!(rendered.priority, Priority::High);
        assert_eq!(rendered.ttl, Some(86400));
    }
}

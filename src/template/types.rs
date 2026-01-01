//! Template types and error definitions

use chrono::{DateTime, Utc};
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

        if !self
            .id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
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

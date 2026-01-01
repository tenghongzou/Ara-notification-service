//! Notification content resolution (direct or template-based)

use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::notification::Priority;
use crate::template::{substitute_variables, TemplateStore};

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
pub struct ResolvedContent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub priority: Priority,
    pub ttl: Option<u32>,
}

impl NotificationContent {
    /// Resolve the content to event_type and payload
    pub fn resolve(
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

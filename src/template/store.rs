//! Template storage with CRUD operations

use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;

use super::substitution::substitute_variables;
use super::types::{
    RenderedTemplate, Template, TemplateError, TemplateResult, UpdateTemplateRequest,
};

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

/// Create an Arc-wrapped template store
pub fn create_template_store() -> Arc<TemplateStore> {
    Arc::new(TemplateStore::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::Priority;
    use serde_json::json;

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

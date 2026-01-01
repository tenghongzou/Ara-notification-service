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

mod store;
mod substitution;
mod types;

pub use store::{create_template_store, TemplateStore};
pub use substitution::substitute_variables;
pub use types::{
    CreateTemplateRequest, RenderedTemplate, Template, TemplateError, TemplateListResponse,
    TemplateResult, UpdateTemplateRequest,
};

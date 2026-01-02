//! API layer - HTTP endpoint handlers organized by domain.

mod cluster;
mod connection;
mod health;
mod metrics;
mod routes;
mod template;
mod tenant;

// Re-export all handlers for use in server/app.rs
pub use cluster::{cluster_status, cluster_user_location};
pub use connection::{get_channel, get_user_subscriptions, list_channels};
pub use connection::{ChannelError, ChannelErrorResponse};
pub use health::{health, stats};
pub use metrics::prometheus_metrics;
pub use routes::api_routes;
pub use template::{create_template, delete_template, get_template, list_templates, update_template};
pub use tenant::{get_tenant_stats, list_tenants};

// Infrastructure layer (shared components)
pub mod infrastructure;

// Re-export infrastructure modules for backward compatibility
pub use infrastructure::auth;
pub use infrastructure::config;
pub use infrastructure::error;
pub use infrastructure::metrics;
pub use infrastructure::postgres;
pub use infrastructure::redis;

// Domain layer (business logic)
pub mod domain;

// Re-export domain modules for backward compatibility
pub use domain::ack;
pub use domain::queue;
pub use domain::template;

// Domain modules (not yet moved to domain/)
pub mod cluster;
pub mod connection_manager;
pub mod notification;
pub mod ratelimit;
pub mod tenant;

// Application layer
pub mod api;
pub mod server;
pub mod sse;
pub mod triggers;
pub mod websocket;

// Supporting modules
pub mod shutdown;
pub mod tasks;
pub mod telemetry;

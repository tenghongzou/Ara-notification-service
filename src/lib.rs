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
pub mod cluster;
pub mod connection_manager;
pub mod notification;
pub mod queue;
pub mod ratelimit;
pub mod template;
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

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
pub use domain::cluster;
pub use domain::connection as connection_manager; // Renamed but re-exported with old name
pub use domain::notification;
pub use domain::queue;
pub use domain::ratelimit;
pub use domain::realtime::sse;
pub use domain::realtime::websocket;
pub use domain::template;
pub use domain::tenant;

// Re-export triggers for backward compatibility
pub use domain::notification::triggers;

// Application layer
pub mod api;
pub mod server;

// Supporting modules
pub mod shutdown;
pub mod tasks;
pub mod telemetry;

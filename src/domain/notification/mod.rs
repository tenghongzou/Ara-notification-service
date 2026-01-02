//! Notification types and dispatching.

mod dispatcher;
mod types;

pub use dispatcher::{DeliveryResult, NotificationDispatcher};
pub use types::{
    Audience, NotificationBuilder, NotificationEvent, NotificationMetadata, NotificationTarget,
    Priority,
};

// Re-export ACK types from domain module for backward compatibility
pub use crate::domain::ack::{
    create_ack_backend, AckConfig, AckStatsSnapshot, AckTracker,
    AckBackendError, AckBackendStats, AckTrackerBackend, PendingAckInfo,
    MemoryAckBackend, PostgresAckBackend, RedisAckBackend,
};

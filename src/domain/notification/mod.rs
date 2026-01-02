//! Notification domain module.
//!
//! This module provides notification dispatching and triggers:
//! - `dispatcher`: Core notification dispatch logic
//! - `types`: Notification event types and builders
//! - `triggers`: HTTP and Redis Pub/Sub notification triggers

mod dispatcher;
mod types;
pub mod triggers;

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

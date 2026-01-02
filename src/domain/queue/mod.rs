//! Message queue module for offline message delivery.
//!
//! This module provides a per-user message queue that stores notifications
//! when users are disconnected and replays them upon reconnection.
//!
//! # Architecture
//!
//! The queue system uses a backend abstraction to support different storage
//! implementations:
//!
//! - `MemoryQueueBackend`: In-memory storage using DashMap (default)
//! - `RedisQueueBackend`: Persistent storage using Redis Streams
//! - `PostgresQueueBackend`: Persistent storage using PostgreSQL
//!
//! Use `create_backend()` to create the appropriate backend based on configuration.

pub mod backend;
mod factory;
pub mod memory_backend;
mod models;
pub mod postgres_backend;
pub mod redis_backend;
mod user_queue;

// Re-export backend types
pub use backend::{
    DrainResult, MessageQueueBackend, QueueBackendError, QueueBackendStats, StoredMessage,
};
pub use factory::create_queue_backend;
pub use memory_backend::MemoryQueueBackend;
pub use models::{QueueConfig, QueueError, QueueStats, QueuedMessage, ReplayResult};
pub use postgres_backend::PostgresQueueBackend;
pub use redis_backend::RedisQueueBackend;
pub use user_queue::UserMessageQueue;

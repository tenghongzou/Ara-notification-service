//! PostgreSQL persistence module.
//!
//! Provides connection pooling and health tracking for PostgreSQL backend.

pub mod pool;

pub use pool::{PostgresPool, PostgresPoolError};

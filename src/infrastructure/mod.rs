//! Infrastructure layer modules
//!
//! This module contains shared infrastructure components:
//! - `auth`: JWT authentication and validation
//! - `config`: Application configuration and settings
//! - `error`: Unified error types
//! - `metrics`: Prometheus metrics helpers
//! - `postgres`: PostgreSQL connection pool
//! - `redis`: Redis connection pool, circuit breaker, and health checks

pub mod auth;
pub mod config;
pub mod error;
pub mod metrics;
pub mod postgres;
pub mod redis;

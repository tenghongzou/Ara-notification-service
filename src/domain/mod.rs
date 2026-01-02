//! Domain layer modules
//!
//! This module contains business domain logic:
//! - `ack`: Delivery acknowledgment tracking
//! - `cluster`: Distributed cluster support
//! - `connection`: Connection management
//! - `notification`: Notification dispatching and triggers
//! - `queue`: Offline message queue
//! - `ratelimit`: Rate limiting
//! - `realtime`: WebSocket and SSE handlers
//! - `template`: Notification templates
//! - `tenant`: Multi-tenant support

pub mod ack;
pub mod cluster;
pub mod connection;
pub mod notification;
pub mod queue;
pub mod ratelimit;
pub mod realtime;
pub mod template;
pub mod tenant;

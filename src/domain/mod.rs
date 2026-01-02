//! Domain layer modules
//!
//! This module contains business domain logic:
//! - `ack`: Delivery acknowledgment tracking
//! - `queue`: Offline message queue
//! - `template`: Notification templates

pub mod ack;
pub mod queue;
pub mod template;

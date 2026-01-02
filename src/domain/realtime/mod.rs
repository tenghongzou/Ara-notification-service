//! Real-time communication domain module.
//!
//! This module provides real-time notification delivery via:
//! - `websocket`: Bidirectional WebSocket connections
//! - `sse`: Server-Sent Events (unidirectional, WebSocket fallback)

pub mod sse;
pub mod websocket;

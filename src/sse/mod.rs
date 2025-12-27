//! Server-Sent Events (SSE) fallback for notification delivery.
//!
//! This module provides an SSE endpoint as a fallback for clients that cannot
//! use WebSocket connections (e.g., due to firewall restrictions or browser limitations).
//!
//! # Features
//!
//! - JWT authentication via query parameter or Authorization header
//! - One-way notification streaming (server to client only)
//! - Heartbeat messages to keep the connection alive
//! - Same connection management as WebSocket (shares ConnectionManager)
//! - Automatic replay of queued messages on connect
//!
//! # Endpoint
//!
//! `GET /sse?token=<JWT_TOKEN>`
//!
//! Or with Authorization header:
//! `GET /sse` with `Authorization: Bearer <JWT_TOKEN>`
//!
//! # Event Types
//!
//! - `notification` - Notification event with JSON payload
//! - `heartbeat` - Keep-alive event with timestamp
//! - `connected` - Initial connection confirmation
//! - `error` - Error event (connection will close after)

mod handler;

pub use handler::{sse_handler, SseQuery};

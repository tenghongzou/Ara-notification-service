//! HTTP notification trigger handlers
//!
//! This module provides HTTP API endpoints for sending notifications:
//! - Single user notifications
//! - Multi-user notifications
//! - Broadcast notifications
//! - Channel notifications
//! - Batch notifications

mod batch;
mod content;
mod handlers;
mod models;

// Re-export handlers
pub use handlers::{
    broadcast_notification, channel_notification, multi_channel_notification, send_notification,
    send_to_users,
};

// Re-export batch
pub use batch::{
    batch_send, BatchItemResult, BatchNotificationItem, BatchOptions, BatchSendRequest,
    BatchSendResponse, BatchSummary, BatchTarget,
};

// Re-export models
pub use models::{
    BroadcastNotificationRequest, ChannelNotificationRequest, MultiChannelNotificationRequest,
    SendNotificationRequest, SendNotificationResponse, SendToUsersRequest,
};

// Re-export content types
pub use content::{NotificationContent, ResolvedContent};

use axum::{
    routing::{get, post},
    Router,
};

use crate::server::AppState;
use crate::triggers::{
    broadcast_notification, channel_notification, multi_channel_notification, send_notification,
    send_to_users,
};

use super::health::{health, stats};

pub fn api_routes() -> Router<AppState> {
    Router::new()
        // Health & Stats
        .route("/health", get(health))
        .route("/stats", get(stats))
        // Notification endpoints
        .nest(
            "/api/v1",
            Router::new()
                // Point-to-point
                .route("/notifications/send", post(send_notification))
                .route("/notifications/send-to-users", post(send_to_users))
                // Broadcast
                .route("/notifications/broadcast", post(broadcast_notification))
                // Channel
                .route("/notifications/channel", post(channel_notification))
                .route("/notifications/channels", post(multi_channel_notification)),
        )
}

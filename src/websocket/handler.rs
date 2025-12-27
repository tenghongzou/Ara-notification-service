use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::auth::Claims;
use crate::cluster::SessionInfo;
use crate::connection_manager::ConnectionHandle;
use crate::metrics::{WsMessageMetrics, WS_CONNECTIONS_CLOSED, WS_CONNECTIONS_OPENED, WS_CONNECTION_DURATION};
use crate::server::AppState;

use super::message::{ClientMessage, OutboundMessage, ServerMessage};

const CHANNEL_BUFFER_SIZE: usize = 32;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
}

/// WebSocket upgrade handler
#[tracing::instrument(
    name = "ws.upgrade",
    skip(ws, state, query, headers),
    fields(has_query_token = query.token.is_some())
)]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
) -> Response {
    // Extract token from query parameter or Authorization header
    let token = extract_token(&query, &headers);

    let token = match token {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                "Missing authentication token",
            )
                .into_response();
        }
    };

    // Validate JWT token
    let claims = match state.jwt_validator.validate(&token) {
        Ok(claims) => claims,
        Err(e) => {
            tracing::warn!(error = %e, "JWT validation failed");
            return (StatusCode::UNAUTHORIZED, "Invalid token").into_response();
        }
    };

    tracing::info!(user_id = %claims.sub, "WebSocket upgrade requested");

    // Upgrade to WebSocket
    ws.on_upgrade(move |socket| handle_socket(socket, state, claims))
}

/// Extract token from query parameter or Authorization header
fn extract_token(query: &WsQuery, headers: &HeaderMap) -> Option<String> {
    // First try query parameter
    if let Some(ref token) = query.token {
        return Some(token.clone());
    }

    // Then try Authorization header
    if let Some(auth_header) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    None
}

/// Handle an established WebSocket connection
#[tracing::instrument(
    name = "ws.connection",
    skip(socket, state, claims),
    fields(
        user_id = %claims.sub,
        otel.kind = "server"
    )
)]
async fn handle_socket(socket: WebSocket, state: AppState, claims: Claims) {
    let user_id = claims.sub.clone();
    let tenant_id = claims.tenant_id().to_string();
    let roles = claims.roles.clone();
    let connection_start = std::time::Instant::now();

    // Create channel for sending messages to this connection
    let (tx, mut rx) = mpsc::channel::<OutboundMessage>(CHANNEL_BUFFER_SIZE);

    // Register connection with limit checking
    let handle = match state.connection_manager.register(user_id.clone(), tenant_id.clone(), roles, tx) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Connection rejected");
            // Send error and close
            let (mut ws_sender, _) = socket.split();
            let error_msg = ServerMessage::error("CONNECTION_LIMIT", e.to_string());
            if let Ok(json) = serde_json::to_string(&error_msg) {
                let _ = ws_sender.send(Message::Text(json.into())).await;
            }
            let _ = ws_sender.close().await;
            return;
        }
    };
    let connection_id = handle.id;

    // Record connection opened metric
    WS_CONNECTIONS_OPENED.inc();

    // Register session in cluster store (for distributed deployments)
    if state.session_store.is_enabled() {
        let session_info = SessionInfo {
            connection_id,
            user_id: user_id.clone(),
            tenant_id: tenant_id.clone(),
            server_id: state.session_store.server_id().to_string(),
            connected_at: chrono::Utc::now().timestamp(),
            channels: vec![],
        };
        if let Err(e) = state.session_store.register_session(&session_info).await {
            tracing::warn!(
                connection_id = %connection_id,
                error = %e,
                "Failed to register session in cluster store"
            );
        }
    }

    tracing::info!(
        connection_id = %connection_id,
        user_id = %user_id,
        tenant_id = %tenant_id,
        "WebSocket connection established"
    );

    // Replay any queued messages for this user
    if state.message_queue.is_enabled() {
        let replay_result = state.message_queue.replay(&user_id, &handle.sender).await;
        if replay_result.replayed > 0 || replay_result.expired > 0 {
            tracing::info!(
                connection_id = %connection_id,
                user_id = %user_id,
                replayed = replay_result.replayed,
                expired = replay_result.expired,
                failed = replay_result.failed,
                "Replayed queued messages on reconnect"
            );
        }
    }

    // Split socket into sender and receiver
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Task for sending messages from channel to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // Convert OutboundMessage to JSON string
            // Pre-serialized messages avoid the serialization cost here
            let text = match msg.to_json() {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to serialize message");
                    continue;
                }
            };

            if ws_sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Task for receiving messages from WebSocket
    let state_clone = state.clone();
    let handle_clone = handle.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(result) = ws_receiver.next().await {
            match result {
                Ok(msg) => {
                    if !process_message(msg, &state_clone, &handle_clone).await {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "WebSocket receive error");
                    break;
                }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {
            tracing::debug!(connection_id = %connection_id, "Send task completed");
        }
        _ = recv_task => {
            tracing::debug!(connection_id = %connection_id, "Receive task completed");
        }
    }

    // Unregister connection
    state.connection_manager.unregister(connection_id).await;

    // Unregister session from cluster store
    if state.session_store.is_enabled() {
        if let Err(e) = state.session_store.unregister_session(connection_id).await {
            tracing::warn!(
                connection_id = %connection_id,
                error = %e,
                "Failed to unregister session from cluster store"
            );
        }
    }

    // Record connection closed and duration metrics
    WS_CONNECTIONS_CLOSED.inc();
    let duration = connection_start.elapsed().as_secs_f64();
    WS_CONNECTION_DURATION.observe(duration);

    tracing::info!(
        connection_id = %connection_id,
        user_id = %user_id,
        duration_secs = duration,
        "WebSocket connection closed"
    );
}

/// Process a received WebSocket message
/// Returns false if the connection should be closed
async fn process_message(
    msg: Message,
    state: &AppState,
    handle: &Arc<ConnectionHandle>,
) -> bool {
    match msg {
        Message::Text(text) => {
            handle.update_activity();

            // Parse client message
            let client_msg: ClientMessage = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse client message");
                    let _ = handle
                        .send(ServerMessage::error("INVALID_MESSAGE", e.to_string()))
                        .await;
                    return true;
                }
            };

            // Handle the message
            handle_client_message(client_msg, state, handle).await;
            true
        }
        Message::Binary(_) => {
            // Binary messages not supported
            let _ = handle
                .send(ServerMessage::error(
                    "UNSUPPORTED_FORMAT",
                    "Binary messages are not supported",
                ))
                .await;
            true
        }
        Message::Ping(_) => {
            handle.update_activity();
            // Axum handles pong automatically, but we update activity
            true
        }
        Message::Pong(_) => {
            handle.update_activity();
            true
        }
        Message::Close(_) => {
            tracing::debug!(connection_id = %handle.id, "Received close frame");
            false
        }
    }
}

/// Handle a parsed client message
#[tracing::instrument(
    name = "ws.message",
    skip(state, handle),
    fields(
        connection_id = %handle.id,
        user_id = %handle.user_id,
        message_type = ?msg
    )
)]
async fn handle_client_message(
    msg: ClientMessage,
    state: &AppState,
    handle: &Arc<ConnectionHandle>,
) {
    match msg {
        ClientMessage::Subscribe { channels } => {
            WsMessageMetrics::record_subscribe();
            handle_subscribe(channels, state, handle).await;
        }
        ClientMessage::Unsubscribe { channels } => {
            WsMessageMetrics::record_unsubscribe();
            handle_unsubscribe(channels, state, handle).await;
        }
        ClientMessage::Ping => {
            WsMessageMetrics::record_ping();
            let _ = handle.send(ServerMessage::Pong).await;
        }
        ClientMessage::Ack { notification_id } => {
            WsMessageMetrics::record_ack();
            handle_ack(notification_id, state, handle).await;
        }
    }
}

/// Handle notification acknowledgment
#[tracing::instrument(
    name = "ws.ack",
    skip(state, handle),
    fields(
        connection_id = %handle.id,
        user_id = %handle.user_id
    )
)]
async fn handle_ack(
    notification_id: uuid::Uuid,
    state: &AppState,
    handle: &Arc<ConnectionHandle>,
) {
    if !state.ack_tracker.is_enabled() {
        // ACK tracking is disabled, ignore
        return;
    }

    let acknowledged = state.ack_tracker.acknowledge(notification_id, &handle.user_id);

    if acknowledged {
        // Send confirmation back to client
        let _ = handle.send(ServerMessage::acked(notification_id)).await;
    } else {
        // Invalid ACK - notification not found or wrong user
        let _ = handle
            .send(ServerMessage::error(
                "INVALID_ACK",
                format!("Unknown or invalid notification: {}", notification_id),
            ))
            .await;
    }
}

/// Handle channel subscription
#[tracing::instrument(
    name = "ws.subscribe",
    skip(state, handle),
    fields(
        connection_id = %handle.id,
        channel_count = channels.len()
    )
)]
async fn handle_subscribe(
    channels: Vec<String>,
    state: &AppState,
    handle: &Arc<ConnectionHandle>,
) {
    let mut subscribed = Vec::new();
    let mut errors = Vec::new();

    for channel in channels {
        // Validate channel name
        if !is_valid_channel_name(&channel) {
            tracing::warn!(
                connection_id = %handle.id,
                channel = %channel,
                "Invalid channel name"
            );
            errors.push(format!("Invalid channel name: {}", channel));
            continue;
        }

        match state
            .connection_manager
            .subscribe_to_channel(handle.id, &channel)
            .await
        {
            Ok(()) => subscribed.push(channel),
            Err(e) => {
                tracing::warn!(
                    connection_id = %handle.id,
                    channel = %channel,
                    error = %e,
                    "Failed to subscribe to channel"
                );
                errors.push(e);
            }
        }
    }

    if !subscribed.is_empty() {
        tracing::info!(
            connection_id = %handle.id,
            channels = ?subscribed,
            "Subscribed to channels"
        );
        let _ = handle.send(ServerMessage::subscribed(subscribed.clone())).await;

        // Update session channels in cluster store
        if state.session_store.is_enabled() {
            let current_channels: Vec<String> = handle.subscriptions.read().await.iter().cloned().collect();
            if let Err(e) = state.session_store.update_session_channels(handle.id, current_channels).await {
                tracing::warn!(
                    connection_id = %handle.id,
                    error = %e,
                    "Failed to update session channels in cluster store"
                );
            }
        }
    }

    // Send error for subscription limit exceeded
    if !errors.is_empty() {
        let _ = handle
            .send(ServerMessage::error("SUBSCRIPTION_ERROR", errors.join("; ")))
            .await;
    }
}

/// Handle channel unsubscription
#[tracing::instrument(
    name = "ws.unsubscribe",
    skip(state, handle),
    fields(
        connection_id = %handle.id,
        channel_count = channels.len()
    )
)]
async fn handle_unsubscribe(
    channels: Vec<String>,
    state: &AppState,
    handle: &Arc<ConnectionHandle>,
) {
    let mut unsubscribed = Vec::new();

    for channel in channels {
        state
            .connection_manager
            .unsubscribe_from_channel(handle.id, &channel)
            .await;
        unsubscribed.push(channel);
    }

    if !unsubscribed.is_empty() {
        tracing::info!(
            connection_id = %handle.id,
            channels = ?unsubscribed,
            "Unsubscribed from channels"
        );
        let _ = handle.send(ServerMessage::unsubscribed(unsubscribed)).await;

        // Update session channels in cluster store
        if state.session_store.is_enabled() {
            let current_channels: Vec<String> = handle.subscriptions.read().await.iter().cloned().collect();
            if let Err(e) = state.session_store.update_session_channels(handle.id, current_channels).await {
                tracing::warn!(
                    connection_id = %handle.id,
                    error = %e,
                    "Failed to update session channels in cluster store"
                );
            }
        }
    }
}

/// Validate channel name
fn is_valid_channel_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }

    // Only allow alphanumeric, dash, underscore, and dot
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_channel_names() {
        assert!(is_valid_channel_name("orders"));
        assert!(is_valid_channel_name("system-alerts"));
        assert!(is_valid_channel_name("user_notifications"));
        assert!(is_valid_channel_name("v1.events"));
        assert!(is_valid_channel_name("Channel123"));
    }

    #[test]
    fn test_invalid_channel_names() {
        assert!(!is_valid_channel_name(""));
        assert!(!is_valid_channel_name("channel with spaces"));
        assert!(!is_valid_channel_name("channel/path"));
        assert!(!is_valid_channel_name("channel@special"));
        // Too long
        assert!(!is_valid_channel_name(&"a".repeat(65)));
    }
}

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
use crate::connection_manager::ConnectionHandle;
use crate::server::AppState;

use super::message::{ClientMessage, ServerMessage};

const CHANNEL_BUFFER_SIZE: usize = 32;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
}

/// WebSocket upgrade handler
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
async fn handle_socket(socket: WebSocket, state: AppState, claims: Claims) {
    let user_id = claims.sub.clone();

    // Create channel for sending messages to this connection
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(CHANNEL_BUFFER_SIZE);

    // Register connection
    let handle = state.connection_manager.register(user_id.clone(), tx);
    let connection_id = handle.id;

    tracing::info!(
        connection_id = %connection_id,
        user_id = %user_id,
        "WebSocket connection established"
    );

    // Split socket into sender and receiver
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Task for sending messages from channel to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = match serde_json::to_string(&msg) {
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
    state.connection_manager.unregister(connection_id);

    tracing::info!(
        connection_id = %connection_id,
        user_id = %user_id,
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
            handle.update_activity().await;

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
            handle.update_activity().await;
            // Axum handles pong automatically, but we update activity
            true
        }
        Message::Pong(_) => {
            handle.update_activity().await;
            true
        }
        Message::Close(_) => {
            tracing::debug!(connection_id = %handle.id, "Received close frame");
            false
        }
    }
}

/// Handle a parsed client message
async fn handle_client_message(
    msg: ClientMessage,
    state: &AppState,
    handle: &Arc<ConnectionHandle>,
) {
    match msg {
        ClientMessage::Subscribe { channels } => {
            handle_subscribe(channels, state, handle).await;
        }
        ClientMessage::Unsubscribe { channels } => {
            handle_unsubscribe(channels, state, handle).await;
        }
        ClientMessage::Ping => {
            let _ = handle.send(ServerMessage::Pong).await;
        }
    }
}

/// Handle channel subscription
async fn handle_subscribe(
    channels: Vec<String>,
    state: &AppState,
    handle: &Arc<ConnectionHandle>,
) {
    let mut subscribed = Vec::new();

    for channel in channels {
        // Validate channel name
        if !is_valid_channel_name(&channel) {
            tracing::warn!(
                connection_id = %handle.id,
                channel = %channel,
                "Invalid channel name"
            );
            continue;
        }

        state
            .connection_manager
            .subscribe_to_channel(handle.id, &channel)
            .await;
        subscribed.push(channel);
    }

    if !subscribed.is_empty() {
        tracing::info!(
            connection_id = %handle.id,
            channels = ?subscribed,
            "Subscribed to channels"
        );
        let _ = handle.send(ServerMessage::subscribed(subscribed)).await;
    }
}

/// Handle channel unsubscription
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

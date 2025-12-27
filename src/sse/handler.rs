//! SSE handler implementation.

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
};
use futures::stream::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::metrics::{WS_CONNECTIONS_CLOSED, WS_CONNECTIONS_OPENED, WS_CONNECTION_DURATION};
use crate::server::AppState;
use crate::websocket::{OutboundMessage, ServerMessage};

/// SSE event types
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SseEvent {
    /// Connection established
    #[serde(rename = "connected")]
    Connected { connection_id: String },
}

/// Query parameters for SSE endpoint
#[derive(Debug, Deserialize)]
pub struct SseQuery {
    pub token: Option<String>,
}

/// SSE upgrade handler
#[tracing::instrument(
    name = "sse.connect",
    skip(state, query, headers),
    fields(has_query_token = query.token.is_some())
)]
pub async fn sse_handler(
    State(state): State<AppState>,
    Query(query): Query<SseQuery>,
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

    let user_id = claims.sub.clone();
    let tenant_id = claims.tenant_id().to_string();
    let roles = claims.roles.clone();

    tracing::info!(user_id = %user_id, tenant_id = %tenant_id, "SSE connection requested");

    // Create channel for sending messages to this connection
    let (tx, rx) = mpsc::channel::<OutboundMessage>(32);

    // Register connection with limit checking
    let handle = match state.connection_manager.register(user_id.clone(), tenant_id.clone(), roles, tx) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "SSE connection rejected");
            return (
                StatusCode::TOO_MANY_REQUESTS,
                format!("Connection limit exceeded: {}", e),
            )
                .into_response();
        }
    };

    let connection_id = handle.id;
    let connection_start = std::time::Instant::now();

    // Record connection opened metric
    WS_CONNECTIONS_OPENED.inc();

    tracing::info!(
        connection_id = %connection_id,
        user_id = %user_id,
        tenant_id = %tenant_id,
        "SSE connection established"
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
                "Replayed queued messages on SSE connect"
            );
        }
    }

    // Create the SSE stream
    let stream = create_sse_stream(
        rx,
        connection_id,
        user_id.clone(),
        state.clone(),
        connection_start,
    );

    // Return SSE response with keep-alive
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(state.settings.websocket.heartbeat_interval))
                .text("heartbeat"),
        )
        .into_response()
}

/// Extract token from query parameter or Authorization header
fn extract_token(query: &SseQuery, headers: &HeaderMap) -> Option<String> {
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

/// Create the SSE event stream
fn create_sse_stream(
    rx: mpsc::Receiver<OutboundMessage>,
    connection_id: uuid::Uuid,
    user_id: String,
    state: AppState,
    connection_start: std::time::Instant,
) -> impl Stream<Item = Result<Event, Infallible>> {
    // Create a cleanup guard that will be dropped when the stream ends
    let cleanup_guard = CleanupGuard::new(
        connection_id,
        user_id.clone(),
        state,
        connection_start,
    );

    // Convert receiver to stream
    let message_stream = ReceiverStream::new(rx);

    // Create initial connected event
    let connected_event = SseEvent::Connected {
        connection_id: connection_id.to_string(),
    };
    let connected_json = serde_json::to_string(&connected_event).unwrap_or_default();

    // Use async_stream to create a simple stream with cleanup
    async_stream::stream! {
        // Emit initial connected event
        yield Ok(Event::default().event("connected").data(connected_json));

        // Hold the cleanup guard - it will be dropped when the stream ends
        let _guard = cleanup_guard;

        // Stream messages
        let mut message_stream = message_stream;
        while let Some(msg) = message_stream.next().await {
            let event = match msg.to_json() {
                Ok(json) => {
                    // Determine event type from message
                    let event_type = match &msg {
                        OutboundMessage::Raw(ServerMessage::Notification { .. }) => "notification",
                        OutboundMessage::Raw(ServerMessage::Heartbeat) => "heartbeat",
                        OutboundMessage::Raw(ServerMessage::Error { .. }) => "error",
                        OutboundMessage::Raw(_) => "message",
                        OutboundMessage::Serialized(_) => "notification",
                    };
                    Event::default().event(event_type).data(json)
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to serialize SSE message");
                    Event::default()
                        .event("error")
                        .data(format!(r#"{{"code":"SERIALIZATION_ERROR","message":"{}"}}"#, e))
                }
            };
            yield Ok(event);
        }
    }
}

/// Guard that performs cleanup when dropped
struct CleanupGuard {
    connection_id: uuid::Uuid,
    user_id: String,
    state: AppState,
    connection_start: std::time::Instant,
}

impl CleanupGuard {
    fn new(
        connection_id: uuid::Uuid,
        user_id: String,
        state: AppState,
        connection_start: std::time::Instant,
    ) -> Self {
        Self {
            connection_id,
            user_id,
            state,
            connection_start,
        }
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        // Record metrics synchronously
        WS_CONNECTIONS_CLOSED.inc();
        let duration = self.connection_start.elapsed().as_secs_f64();
        WS_CONNECTION_DURATION.observe(duration);

        tracing::info!(
            connection_id = %self.connection_id,
            user_id = %self.user_id,
            duration_secs = duration,
            "SSE connection closed"
        );

        // Spawn a task to unregister the connection (async operation)
        let connection_manager = self.state.connection_manager.clone();
        let connection_id = self.connection_id;
        tokio::spawn(async move {
            connection_manager.unregister(connection_id).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_serialization() {
        let connected = SseEvent::Connected {
            connection_id: "test-123".to_string(),
        };
        let json = serde_json::to_string(&connected).unwrap();
        assert!(json.contains(r#""type":"connected""#));
        assert!(json.contains(r#""connection_id":"test-123""#));
    }

    #[test]
    fn test_extract_token_from_query() {
        let query = SseQuery {
            token: Some("my-token".to_string()),
        };
        let headers = HeaderMap::new();
        assert_eq!(extract_token(&query, &headers), Some("my-token".to_string()));
    }

    #[test]
    fn test_extract_token_from_header() {
        let query = SseQuery { token: None };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            "Bearer header-token".parse().unwrap(),
        );
        assert_eq!(extract_token(&query, &headers), Some("header-token".to_string()));
    }

    #[test]
    fn test_extract_token_query_takes_precedence() {
        let query = SseQuery {
            token: Some("query-token".to_string()),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            "Bearer header-token".parse().unwrap(),
        );
        assert_eq!(extract_token(&query, &headers), Some("query-token".to_string()));
    }

    #[test]
    fn test_extract_token_none() {
        let query = SseQuery { token: None };
        let headers = HeaderMap::new();
        assert_eq!(extract_token(&query, &headers), None);
    }
}

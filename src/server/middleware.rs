use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use super::AppState;
use crate::metrics::RateLimitMetrics;
use crate::ratelimit::RateLimitResult;

/// API Key authentication middleware
/// Validates X-API-Key header against configured api.key
pub async fn api_key_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // If no API key is configured, allow all requests (development mode)
    let Some(expected_key) = &state.settings.api.key else {
        return Ok(next.run(req).await);
    };

    // Check X-API-Key header
    let api_key = req
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok());

    match api_key {
        Some(key) if key == expected_key => Ok(next.run(req).await),
        Some(_) => {
            tracing::warn!("Invalid API key provided");
            Err(StatusCode::UNAUTHORIZED)
        }
        None => {
            tracing::warn!("Missing API key header");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Extract bearer token from Authorization header
pub fn extract_bearer_token(req: &Request<Body>) -> Option<&str> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// Rate limiting middleware for HTTP API requests.
///
/// Uses API key or IP address as the rate limit key.
/// Returns 429 Too Many Requests with Retry-After header when rate limited.
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Skip if rate limiting is disabled
    if !state.rate_limiter.is_enabled() {
        return next.run(req).await;
    }

    // Get API key from header or use IP address
    let api_key = req
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok());

    let result = state.rate_limiter.check_http(api_key, addr.ip());

    match result {
        RateLimitResult::Allowed {
            remaining,
            limit,
            reset_at,
        } => {
            RateLimitMetrics::record_http_allowed();
            let mut response = next.run(req).await;

            // Add rate limit headers to response
            let headers = response.headers_mut();
            if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
                headers.insert("X-RateLimit-Limit", v);
            }
            if let Ok(v) = HeaderValue::from_str(&remaining.to_string()) {
                headers.insert("X-RateLimit-Remaining", v);
            }
            if let Ok(v) = HeaderValue::from_str(&reset_at.to_string()) {
                headers.insert("X-RateLimit-Reset", v);
            }

            response
        }
        RateLimitResult::Denied {
            retry_after,
            limit,
            reset_at,
        } => {
            RateLimitMetrics::record_http_denied();
            tracing::warn!(
                ip = %addr.ip(),
                api_key = ?api_key,
                retry_after = retry_after,
                "Rate limit exceeded"
            );

            rate_limit_response(retry_after, limit, reset_at)
        }
    }
}

/// Rate limiting middleware for WebSocket connections.
///
/// Limits connection rate per IP address.
pub async fn ws_rate_limit_middleware(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Skip if rate limiting is disabled
    if !state.rate_limiter.is_enabled() {
        return next.run(req).await;
    }

    let result = state.rate_limiter.check_ip(addr.ip());

    match result {
        RateLimitResult::Allowed { .. } => {
            RateLimitMetrics::record_ws_allowed();
            next.run(req).await
        }
        RateLimitResult::Denied {
            retry_after,
            limit,
            reset_at,
        } => {
            RateLimitMetrics::record_ws_denied();
            tracing::warn!(
                ip = %addr.ip(),
                retry_after = retry_after,
                "WebSocket connection rate limit exceeded"
            );

            rate_limit_response(retry_after, limit, reset_at)
        }
    }
}

/// Build a rate limit error response with proper headers
fn rate_limit_response(retry_after: u64, limit: u32, reset_at: i64) -> Response {
    let body = json!({
        "error": {
            "code": "RATE_LIMITED",
            "message": format!("Too many requests, please retry after {} seconds", retry_after)
        }
    });

    let mut response = (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();

    let headers = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
        headers.insert("Retry-After", v);
    }
    if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
        headers.insert("X-RateLimit-Limit", v);
    }
    if let Ok(v) = HeaderValue::from_str("0") {
        headers.insert("X-RateLimit-Remaining", v);
    }
    if let Ok(v) = HeaderValue::from_str(&reset_at.to_string()) {
        headers.insert("X-RateLimit-Reset", v);
    }

    response
}

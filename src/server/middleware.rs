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
use crate::tenant::TenantContext;

/// Tenant context extracted from HTTP request, stored in request extensions
#[derive(Clone, Debug)]
pub struct RequestTenantContext(pub TenantContext);

impl RequestTenantContext {
    /// Get the tenant ID
    pub fn tenant_id(&self) -> &str {
        &self.0.tenant_id
    }

    /// Namespace a channel name for tenant isolation
    pub fn namespace_channel(&self, channel: &str) -> String {
        self.0.namespace_channel(channel)
    }
}

/// Constant-time string comparison to prevent timing attacks.
/// Always compares all bytes regardless of where they differ.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let result = a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    result == 0
}

/// API Key authentication middleware
/// Validates X-API-Key header against configured api.key
pub async fn api_key_auth(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let is_production = state.settings.is_production;

    // If no API key is configured, only allow in non-production mode.
    let Some(expected_key) = &state.settings.api.key else {
        if is_production {
            tracing::error!(
                "🚨 [P0] Production API key misconfiguration: API_KEY is missing, rejecting request"
            );
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        return Ok(next.run(req).await);
    };

    // Check X-API-Key header
    let api_key = req.headers().get("X-API-Key").and_then(|v| v.to_str().ok());

    match api_key {
        Some(key) if constant_time_eq(key.as_bytes(), expected_key.as_bytes()) => {
            // Extract tenant context from X-Tenant-ID header when multi-tenancy is enabled
            if state.tenant_manager.is_enabled() {
                let tenant_id = req
                    .headers()
                    .get("X-Tenant-ID")
                    .and_then(|v| v.to_str().ok());

                match tenant_id.map(str::trim) {
                    Some(tid) if is_valid_tenant_id(tid) => {
                        let ctx = state.tenant_manager.create_context(tid);
                        req.extensions_mut().insert(RequestTenantContext(ctx));
                    }
                    Some(_) => {
                        tracing::warn!("Invalid X-Tenant-ID header format");
                        return Err(StatusCode::BAD_REQUEST);
                    }
                    None => {
                        tracing::warn!("Missing X-Tenant-ID header with multi-tenancy enabled");
                        return Err(StatusCode::BAD_REQUEST);
                    }
                }
            }

            Ok(next.run(req).await)
        }
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
    let api_key = req.headers().get("X-API-Key").and_then(|v| v.to_str().ok());

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

/// Validate tenant ID format.
/// Must be non-empty, max 64 chars, alphanumeric + dash + underscore + dot (no colon).
/// Colon is excluded to prevent channel namespace collisions.
fn is_valid_tenant_id(id: &str) -> bool {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return false;
    }
    trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
}

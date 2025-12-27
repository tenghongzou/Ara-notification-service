use axum::{middleware, routing::get, Router};
use tower_http::{
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};

use crate::sse::sse_handler;
use crate::websocket::ws_handler;

use super::middleware::{api_key_auth, rate_limit_middleware, ws_rate_limit_middleware};
use super::AppState;

/// Maximum request body size for regular endpoints (64 KB)
const MAX_BODY_SIZE: usize = 64 * 1024;

/// Maximum request body size for batch endpoint (1 MB)
const MAX_BATCH_BODY_SIZE: usize = 1024 * 1024;

pub fn create_app(state: AppState) -> Router {
    // CORS configuration - use configured origins or allow any in development
    let cors = build_cors_layer(&state.settings.server.cors_origins);

    // WebSocket and SSE routes with connection rate limiting
    let ws_routes = Router::new()
        .route("/ws", get(ws_handler))
        .route("/sse", get(sse_handler))
        .layer(middleware::from_fn_with_state(state.clone(), ws_rate_limit_middleware));

    // Health check and metrics (no rate limiting, no auth)
    let health_routes = Router::new()
        .route("/health", get(crate::api::health))
        .route("/metrics", get(crate::api::prometheus_metrics));

    // Regular notification routes (64KB limit)
    let notification_routes = Router::new()
        .route("/notifications/send", axum::routing::post(crate::triggers::send_notification))
        .route("/notifications/send-to-users", axum::routing::post(crate::triggers::send_to_users))
        .route("/notifications/broadcast", axum::routing::post(crate::triggers::broadcast_notification))
        .route("/notifications/channel", axum::routing::post(crate::triggers::channel_notification))
        .route("/notifications/channels", axum::routing::post(crate::triggers::multi_channel_notification))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE));

    // Batch notification route (1MB limit)
    let batch_routes = Router::new()
        .route("/notifications/batch", axum::routing::post(crate::triggers::batch_send))
        .layer(RequestBodyLimitLayer::new(MAX_BATCH_BODY_SIZE));

    // Channel info routes (read-only, no body limit needed)
    let channel_routes = Router::new()
        .route("/channels", get(crate::api::list_channels))
        .route("/channels/{name}", get(crate::api::get_channel))
        .route("/users/{user_id}/subscriptions", get(crate::api::get_user_subscriptions));

    // Template CRUD routes
    let template_routes = Router::new()
        .route("/templates", axum::routing::post(crate::api::create_template))
        .route("/templates", get(crate::api::list_templates))
        .route("/templates/{id}", get(crate::api::get_template))
        .route("/templates/{id}", axum::routing::put(crate::api::update_template))
        .route("/templates/{id}", axum::routing::delete(crate::api::delete_template));

    // Tenant management routes
    let tenant_routes = Router::new()
        .route("/tenants", get(crate::api::list_tenants))
        .route("/tenants/{tenant_id}", get(crate::api::get_tenant_stats));

    // Cluster management routes
    let cluster_routes = Router::new()
        .route("/cluster/status", get(crate::api::cluster_status))
        .route("/cluster/users/{user_id}", get(crate::api::cluster_user_location));

    // Protected API routes (require API key) with rate limiting
    let protected_routes = Router::new()
        .route("/stats", get(crate::api::stats))
        .nest("/api/v1", notification_routes.merge(batch_routes).merge(channel_routes).merge(template_routes).merge(tenant_routes).merge(cluster_routes))
        .layer(middleware::from_fn_with_state(state.clone(), api_key_auth))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware));

    Router::new()
        .merge(ws_routes)
        .merge(health_routes)
        .merge(protected_routes)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// Build CORS layer from configured origins
fn build_cors_layer(origins: &[String]) -> CorsLayer {
    use tower_http::cors::AllowOrigin;

    if origins.is_empty() {
        // Development mode: allow any origin (with warning)
        tracing::warn!("CORS: No origins configured, allowing any origin. Configure CORS_ORIGINS for production.");
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        // Production mode: restrict to configured origins
        let origins: Vec<_> = origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();

        tracing::info!("CORS: Restricting to {} configured origins", origins.len());
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::CONTENT_TYPE,
                axum::http::header::AUTHORIZATION,
                axum::http::header::HeaderName::from_static("x-api-key"),
            ])
    }
}

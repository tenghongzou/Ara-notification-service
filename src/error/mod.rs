use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

/// Check if running in production mode (based on RUN_MODE env var)
fn is_production() -> bool {
    std::env::var("RUN_MODE")
        .map(|m| m == "production" || m == "prod")
        .unwrap_or(false)
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, client_message, log_message) = match &self {
            AppError::Config(e) => {
                let log_msg = e.to_string();
                let client_msg = if is_production() {
                    "Configuration error".to_string()
                } else {
                    log_msg.clone()
                };
                (StatusCode::INTERNAL_SERVER_ERROR, "CONFIG_ERROR", client_msg, log_msg)
            }
            AppError::Auth(msg) => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                msg.clone(),
                msg.clone(),
            ),
            AppError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                msg.clone(),
                msg.clone(),
            ),
            AppError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                msg.clone(),
                msg.clone(),
            ),
            AppError::Internal(e) => {
                let log_msg = e.clone();
                let client_msg = if is_production() {
                    "Internal server error".to_string()
                } else {
                    log_msg.clone()
                };
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", client_msg, log_msg)
            }
            AppError::Redis(e) => {
                let log_msg = e.to_string();
                let client_msg = if is_production() {
                    "Service temporarily unavailable".to_string()
                } else {
                    log_msg.clone()
                };
                (StatusCode::INTERNAL_SERVER_ERROR, "REDIS_ERROR", client_msg, log_msg)
            }
        };

        // Always log the detailed error server-side
        tracing::error!(
            code = %code,
            status = %status.as_u16(),
            message = %log_message,
            "API error"
        );

        let body = ErrorResponse {
            error: ErrorBody {
                code: code.to_string(),
                message: client_message,
            },
        };

        (status, Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

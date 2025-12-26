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

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::Config(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "CONFIG_ERROR",
                e.to_string(),
            ),
            AppError::Auth(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg.clone()),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            AppError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                msg.clone(),
            ),
            AppError::Redis(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "REDIS_ERROR",
                e.to_string(),
            ),
        };

        let body = ErrorResponse {
            error: ErrorBody {
                code: code.to_string(),
                message,
            },
        };

        (status, Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    #[allow(dead_code)]
    NotFound(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Internal server error: {0}")]
    #[allow(dead_code)]
    Internal(String),

    #[error("NATS error: {0}")]
    Nats(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("UUID error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("Time parse error: {0}")]
    TimeParse(#[from] chrono::ParseError),

    #[error("Configuration error: {0}")]
    Config(#[from] anyhow::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Semaphore error: {0}")]
    Semaphore(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message, should_log) = match &self {
            AppError::Database(_e) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error", true),
            AppError::Validation(message) => (StatusCode::BAD_REQUEST, message.as_str(), false),
            AppError::NotFound(message) => (StatusCode::NOT_FOUND, message.as_str(), false),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized", false),
            AppError::Internal(message) => {
                (StatusCode::INTERNAL_SERVER_ERROR, message.as_str(), true)
            }
            AppError::Nats(_e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Message queue error",
                true,
            ),
            AppError::Json(_e) => (StatusCode::BAD_REQUEST, "Invalid JSON", false),
            AppError::Uuid(_e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "UUID generation error",
                true,
            ),
            AppError::TimeParse(_e) => (StatusCode::BAD_REQUEST, "Invalid time format", false),
            AppError::Config(_e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Configuration error",
                true,
            ),
            AppError::Io(_e) => (StatusCode::INTERNAL_SERVER_ERROR, "IO error", true),
            AppError::Semaphore(_e) => (StatusCode::INTERNAL_SERVER_ERROR, "Semaphore error", true),
        };

        if should_log {
            tracing::error!("애플리케이션 오류: {}", self);
        }

        let body = Json(json!({
            "error": error_message,
            "timestamp": chrono::Utc::now(),
        }));

        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

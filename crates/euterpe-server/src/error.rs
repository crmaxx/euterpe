use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use euterpe_qobuz::QobuzError;

use crate::api::ErrorBody;
use crate::api::ErrorResponse;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("{0}")]
    Message(String),
    #[error("qobuz error: {0}")]
    Qobuz(#[from] QobuzError),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

impl ApiError {
    pub fn code(&self) -> &'static str {
        match self {
            ApiError::Config(_) => "CONFIG_ERROR",
            ApiError::Message(msg) if msg.contains("MASTER_KEY") => "MASTER_KEY_REQUIRED",
            ApiError::Message(msg)
                if msg.contains("credentials") || msg.contains("not connected") =>
            {
                "QOBUZ_NOT_CONFIGURED"
            }
            ApiError::Message(msg) if msg.contains("JOB_ALREADY_RUNNING") => "JOB_ALREADY_RUNNING",
            ApiError::Message(msg) if msg.contains("SCAN_ALREADY_RUNNING") => {
                "SCAN_ALREADY_RUNNING"
            }
            ApiError::Message(msg) if msg.contains("not found") => "NOT_FOUND",
            ApiError::Message(msg) if msg.contains("cannot cancel") => "JOB_NOT_CANCELLABLE",
            ApiError::Message(msg) if msg.contains("cannot purge") => "JOB_NOT_PURGEABLE",
            ApiError::Message(msg) if msg.contains("TORRENT_SESSION_BUSY") => {
                "TORRENT_SESSION_BUSY"
            }
            ApiError::Message(msg) if msg.contains("INVALID_CURSOR") => "INVALID_CURSOR",
            ApiError::Message(msg) if msg.contains("PROVIDER_UNAVAILABLE") => {
                "PROVIDER_UNAVAILABLE"
            }
            ApiError::Message(msg) if msg.contains("integration not found") => "NOT_FOUND",
            ApiError::Message(msg) if msg.contains("PAYLOAD_TOO_LARGE") => "PAYLOAD_TOO_LARGE",
            ApiError::Qobuz(QobuzError::Authentication(_)) => "QOBUZ_AUTH_FAILED",
            ApiError::Qobuz(_) => "QOBUZ_UNAVAILABLE",
            ApiError::Message(_) => "BAD_REQUEST",
            ApiError::Db(_) => "DATABASE_ERROR",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            ApiError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Message(msg) if msg.contains("MASTER_KEY") => StatusCode::BAD_REQUEST,
            ApiError::Message(msg)
                if msg.contains("credentials") || msg.contains("not connected") =>
            {
                StatusCode::SERVICE_UNAVAILABLE
            }
            ApiError::Message(msg) if msg.contains("JOB_ALREADY_RUNNING") => StatusCode::CONFLICT,
            ApiError::Message(msg) if msg.contains("SCAN_ALREADY_RUNNING") => StatusCode::CONFLICT,
            ApiError::Message(msg) if msg.contains("not found") => StatusCode::NOT_FOUND,
            ApiError::Message(msg) if msg.contains("cannot cancel") => StatusCode::CONFLICT,
            ApiError::Message(msg) if msg.contains("cannot purge") => StatusCode::CONFLICT,
            ApiError::Message(msg) if msg.contains("TORRENT_SESSION_BUSY") => StatusCode::CONFLICT,
            ApiError::Message(msg) if msg.contains("PROVIDER_UNAVAILABLE") => {
                StatusCode::BAD_GATEWAY
            }
            ApiError::Message(msg) if msg.contains("PAYLOAD_TOO_LARGE") => {
                StatusCode::PAYLOAD_TOO_LARGE
            }
            ApiError::Qobuz(QobuzError::Authentication(_)) => StatusCode::UNAUTHORIZED,
            ApiError::Qobuz(_) => StatusCode::BAD_GATEWAY,
            ApiError::Message(_) => StatusCode::BAD_REQUEST,
            ApiError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError::Message(message.into())
    }

    pub fn invalid_cursor(message: impl Into<String>) -> Self {
        ApiError::Message(format!("INVALID_CURSOR: {}", message.into()))
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        ApiError::Message(format!("PAYLOAD_TOO_LARGE: {}", message.into()))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorResponse {
            error: ErrorBody {
                code: self.code().to_string(),
                message: self.to_string(),
            },
        };
        (status, Json(body)).into_response()
    }
}

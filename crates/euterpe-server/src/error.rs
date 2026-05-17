use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
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
            ApiError::Message(msg) if msg.contains("credentials") => "QOBUZ_NOT_CONFIGURED",
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
            ApiError::Message(msg) if msg.contains("credentials") => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::Qobuz(QobuzError::Authentication(_)) => StatusCode::UNAUTHORIZED,
            ApiError::Qobuz(_) => StatusCode::BAD_GATEWAY,
            ApiError::Message(_) => StatusCode::BAD_REQUEST,
            ApiError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError::Message(message.into())
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

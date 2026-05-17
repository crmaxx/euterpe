use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::api::{ErrorBody, ErrorResponse};
use crate::config::AppConfig;

pub async fn admin_auth(
    State(config): State<Arc<AppConfig>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let Some(expected) = config.admin_password.as_deref() else {
        return Ok(next.run(request).await);
    };

    let authorized = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|h| check_auth(h, expected))
        .unwrap_or(false);

    if authorized {
        Ok(next.run(request).await)
    } else {
        let body = ErrorResponse {
            error: ErrorBody {
                code: "UNAUTHORIZED".into(),
                message: "admin authentication required".into(),
            },
        };
        Err((StatusCode::UNAUTHORIZED, Json(body)).into_response())
    }
}

fn check_auth(header: &str, expected: &str) -> bool {
    if let Some(token) = header.strip_prefix("Bearer ") {
        return token == expected;
    }
    if let Some(encoded) = header.strip_prefix("Basic ") {
        use base64::Engine;
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded) {
            if let Ok(s) = String::from_utf8(decoded) {
                return s == format!("admin:{expected}") || s == expected;
            }
        }
    }
    false
}

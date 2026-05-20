use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

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

    let authorized = check_request_auth(&request, expected);

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

fn check_request_auth(request: &Request<Body>, expected: &str) -> bool {
    if let Some(h) = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && check_auth(h, expected)
    {
        return true;
    }

    if path_allows_query_token(request.uri().path())
        && query_access_token_matches(request.uri().query(), expected)
    {
        return true;
    }

    false
}

/// Endpoints used by browser APIs that cannot set `Authorization` (EventSource, `<audio>`).
fn path_allows_query_token(path: &str) -> bool {
    path.ends_with("/stream") || path == "/api/v1/events"
}

fn query_access_token_matches(query: Option<&str>, expected: &str) -> bool {
    let Some(query) = query else {
        return false;
    };
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        if key == "access_token" && value == expected {
            return true;
        }
    }
    false
}

fn check_auth(header: &str, expected: &str) -> bool {
    if let Some(token) = header.strip_prefix("Bearer ") {
        return token == expected;
    }
    if let Some(encoded) = header.strip_prefix("Basic ") {
        use base64::Engine;
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded)
            && let Ok(s) = String::from_utf8(decoded)
        {
            return s == format!("admin:{expected}") || s == expected;
        }
    }
    false
}

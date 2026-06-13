use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::Uri;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::Level;

use crate::api::{ErrorBody, ErrorResponse};
use crate::config::AppConfig;

const MAX_ERROR_RESPONSE_BODY: usize = 16 * 1024;

/// HTTP access-log line for error responses (4xx/5xx).
///
/// - **5xx** → `ERROR`
/// - **4xx** → `ERROR` (client/validation failures are logged as errors, not warnings)
/// - **2xx/3xx** → `DEBUG` via [`log_http_success`]
pub fn log_http_response(status: u16, latency_ms: u64, error: Option<&ErrorBody>) {
    match status {
        500..=599 => {
            if let Some(error) = error {
                tracing::event!(
                    Level::ERROR,
                    status,
                    latency_ms,
                    code = %error.code,
                    message = %error.message,
                    "http response: server error"
                );
            } else {
                tracing::event!(
                    Level::ERROR,
                    status,
                    latency_ms,
                    "http response: server error"
                );
            }
        }
        400..=499 => {
            if let Some(error) = error {
                tracing::event!(
                    Level::ERROR,
                    status,
                    latency_ms,
                    code = %error.code,
                    message = %error.message,
                    "http response: client error"
                );
            } else {
                tracing::event!(
                    Level::ERROR,
                    status,
                    latency_ms,
                    "http response: client error"
                );
            }
        }
        _ => log_http_success(status, latency_ms),
    }
}

pub fn log_http_success(status: u16, latency_ms: u64) {
    tracing::event!(Level::DEBUG, status, latency_ms, "http response");
}

pub fn request_log_uri(uri: &Uri) -> &str {
    uri.path()
}

/// When `EUTERPE_DEBUG` is on, parse JSON [`ErrorResponse`] and log `code` / `message`.
pub async fn log_http_error_response(
    debug: bool,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let start = Instant::now();
    let response = next.run(request).await;
    let latency_ms = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();
    if status < 400 {
        return response;
    }
    if debug {
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, MAX_ERROR_RESPONSE_BODY)
            .await
            .unwrap_or_default();
        let error = serde_json::from_slice::<ErrorResponse>(&bytes)
            .ok()
            .map(|r| r.error);
        log_http_response(status, latency_ms, error.as_ref());
        Response::from_parts(parts, Body::from(bytes))
    } else {
        log_http_response(status, latency_ms, None);
        response
    }
}

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
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_access_token_decodes_url_encoded_value() {
        let expected = "tok&en=value% with space";
        assert!(query_access_token_matches(
            Some("access_token=tok%26en%3Dvalue%25+with+space"),
            expected
        ));
    }

    #[test]
    fn query_access_token_does_not_match_encoded_bytes() {
        assert!(!query_access_token_matches(
            Some("access_token=tok%26en"),
            "tok%26en"
        ));
    }

    #[test]
    fn request_log_uri_uses_path_without_query() {
        let uri: Uri = "/api/v1/events?access_token=secret&cursor=1"
            .parse()
            .unwrap();

        assert_eq!(request_log_uri(&uri), "/api/v1/events");
    }
}

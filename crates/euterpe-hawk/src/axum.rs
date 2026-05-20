//! Axum / Tower integration: per-request scope, addons, panic reporting.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_http::catch_panic::CatchPanicLayer;

use crate::catcher::{CatchOpts, Hawk};
use crate::event::EventLevel;
use crate::filter::sanitize_header_map;
use crate::http_addons::panic_mechanism_addon;
use crate::panic_flag::mark_panic_reported;
use crate::scope::{self, HawkScope};

/// Middleware: per-request Hawk scope (method, uri, sanitized headers).
pub async fn request_context_middleware(request: Request, next: Next) -> Response {
    let scope = HawkScope::new_http(
        request.method().to_string(),
        request.uri().to_string(),
        sanitize_header_map(
            request
                .headers()
                .iter()
                .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string()))),
        ),
    );
    scope::with_scope(scope, async { next.run(request).await }).await
}

/// Layer that reports handler panics to Hawk and returns HTTP 500.
pub fn catch_panic_layer(
    hawk: Arc<Hawk>,
) -> CatchPanicLayer<impl Fn(Box<dyn std::any::Any + Send>) -> Response + Clone> {
    CatchPanicLayer::custom(move |panic_payload: Box<dyn std::any::Any + Send>| {
        mark_panic_reported();
        let message = if let Some(s) = panic_payload.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic_payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "handler panic".to_string()
        };
        let mut addons = scope::current_http_addons().unwrap_or_else(|| serde_json::json!({}));
        if let serde_json::Value::Object(ref mut map) = addons {
            if let serde_json::Value::Object(m) = panic_mechanism_addon() {
                map.extend(m);
            }
        } else {
            addons = panic_mechanism_addon();
        }
        hawk.catch_message(
            message,
            "panic",
            CatchOpts {
                level: EventLevel::Fatal,
                urgent: true,
                addons: Some(addons),
                ..Default::default()
            },
        );
        (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
    })
}

/// Apply Hawk Axum middleware and panic catching to a router.
pub fn apply_layers(router: axum::Router, hawk: Arc<Hawk>) -> axum::Router {
    router
        .layer(catch_panic_layer(hawk))
        .layer(axum::middleware::from_fn(request_context_middleware))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::get;
    use tower::ServiceExt;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SAMPLE_TOKEN: &str = "eyJpbnRlZ3JhdGlvbklkIjoiZGRjZmY4OTItODMzMy00YjVlLWIyYWQtZWM1MDQ5MDVjMjFlIiwic2VjcmV0IjoiZmJjYzIwMTEtMTY5My00NDIyLThiNDItZDRlMzdlYmI4NWIwIn0=";

    async fn panicking_handler() -> &'static str {
        panic!("handler boom");
    }

    #[tokio::test]
    async fn axum_panic_sends_single_event() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let mut config = crate::config::HawkConfig::from_token(SAMPLE_TOKEN).unwrap();
        config.collector_endpoint = server.uri();
        config.batch_max = 1;
        config.batch_interval = std::time::Duration::from_millis(10);

        let hawk = Hawk::try_new(config).unwrap();
        let app = apply_layers(Router::new().route("/", get(panicking_handler)), hawk);

        let _ = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        server.verify().await;
    }
}

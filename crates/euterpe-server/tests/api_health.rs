use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[path = "support/schema.rs"]
mod schema;

#[tokio::test]
async fn health_returns_ok() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let spec = schema::load_spec();
    schema::validate_schema(
        &schema::schema_from_spec(&spec, "HealthResponse"),
        &json,
    );
    assert_eq!(json["status"], "ok");
}

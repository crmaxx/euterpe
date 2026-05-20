use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

mod support;

#[tokio::test]
async fn static_spa_fallback_serves_index() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("index.html"),
        "<!DOCTYPE html><html><body>Euterpe</body></html>",
    )
    .unwrap();

    let state = app::test_support::test_state().await;
    let mut config = (*state.config).clone();
    config.static_dir = dir.path().to_path_buf();
    let state = euterpe_server::AppState::new(
        config,
        state.db.clone(),
        state.job_tx.clone(),
        state.events.clone(),
        state.scan_events.clone(),
        None,
    )
    .await
    .unwrap();

    let app = app::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/favorites")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("Euterpe"));
}

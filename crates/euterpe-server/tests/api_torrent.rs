use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use euterpe_server::app::test_support::test_state;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn torrent_settings_get_defaults() {
    let state = test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settings/torrent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["seed_ratio_limit"], 0.0);
    assert_eq!(json["seed_time_limit_sec"], 0);
}

#[tokio::test]
async fn torrent_settings_patch_rejects_nonzero_seed_limits() {
    let state = test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/torrent")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"seed_ratio_limit":1.5}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn torrent_settings_patch_accepts_download_only_preset() {
    let state = test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/torrent")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"max_upload_kib_per_sec":128}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["max_upload_kib_per_sec"], 128);
}

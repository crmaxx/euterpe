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
    assert_eq!(json["disable_upload"], true);
    assert_eq!(json["max_upload_kib_per_sec"], 0);
}

#[tokio::test]
async fn torrent_settings_patch_disable_upload_and_max_upload() {
    let state = test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/torrent")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"disable_upload":false,"max_upload_kib_per_sec":128}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["disable_upload"], false);
    assert_eq!(json["max_upload_kib_per_sec"], 128);
}

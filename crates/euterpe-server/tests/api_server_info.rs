use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[path = "support/schema.rs"]
mod schema;

#[tokio::test]
async fn server_info_returns_config_snapshot() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/server/info")
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
        &schema::schema_from_spec(&spec, "ServerInfoResponse"),
        &json,
    );
    assert!(json["library_path"].is_string());
    assert!(json["credentials_configured"].is_boolean());
    assert!(json["admin_auth_required"].is_boolean());
}

#[tokio::test]
async fn sync_latest_returns_null_when_no_runs() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/sync/latest")
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
        &schema::schema_from_spec(&spec, "QobuzSyncLatestResponse"),
        &json,
    );
    assert!(json["run"].is_null());
}

#[tokio::test]
async fn sync_latest_returns_most_recent_run() {
    let state = app::test_support::test_state().await;
    sqlx::query(
        r#"
        INSERT INTO qobuz_sync_runs (started_at, finished_at, status, albums_total, albums_added, albums_removed)
        VALUES ('2020-01-01T00:00:00Z', '2020-01-01T00:01:00Z', 'success', 10, 1, 0)
        "#,
    )
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO qobuz_sync_runs (started_at, status)
        VALUES ('2021-01-01T00:00:00Z', 'running')
        "#,
    )
    .execute(&state.db)
    .await
    .unwrap();

    let app = app::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/sync/latest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["run"]["status"], "running");
    assert_eq!(json["run"]["started_at"], "2021-01-01T00:00:00Z");
}

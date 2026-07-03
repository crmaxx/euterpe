use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_data::repositories::catalog::{self, AlbumUpsert};
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[path = "support/qobuz_mock.rs"]
mod qobuz_mock;
#[path = "support/schema.rs"]
mod schema;

use qobuz_mock::{MockQobuz, state_with_mock};

#[tokio::test]
async fn sync_without_credentials_returns_503() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn scheduled_sync_settings_defaults_match_openapi_contract() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settings/qobuz-scheduled-sync")
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
        &schema::schema_from_spec(&spec, "QobuzScheduledSyncSettingsResponse"),
        &json,
    );
    assert_eq!(json["settings"]["enabled"], false);
    assert_eq!(json["settings"]["cron_expression"], "0 3 * * *");
    assert_eq!(json["settings"]["auto_download_new_favorites"], false);
    assert!(json["status"]["server_timezone"].as_str().is_some());
    assert!(json["status"]["next_run_at"].is_null());
    assert!(json["status"]["last_run"].is_null());
}

#[tokio::test]
async fn scheduled_sync_settings_can_enable_with_default_cron() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/qobuz-scheduled-sync")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"enabled":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["enabled"], true);
    assert_eq!(json["settings"]["cron_expression"], "0 3 * * *");
    assert!(json["status"]["next_run_at"].as_str().is_some());
}

#[tokio::test]
async fn scheduled_sync_settings_normalizes_empty_cron_when_enabling() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/qobuz-scheduled-sync")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"enabled":true,"cron_expression":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["enabled"], true);
    assert_eq!(json["settings"]["cron_expression"], "0 3 * * *");
    assert!(json["status"]["next_run_at"].as_str().is_some());
}

#[tokio::test]
async fn scheduled_sync_settings_reject_invalid_enabled_cron() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/qobuz-scheduled-sync")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"enabled":true,"cron_expression":"not a cron"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn scheduled_sync_settings_patch_accepts_valid_cron_and_returns_next_run() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/qobuz-scheduled-sync")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"enabled":true,"cron_expression":"0 3 * * *","auto_download_new_favorites":true}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["enabled"], true);
    assert_eq!(json["settings"]["cron_expression"], "0 3 * * *");
    assert_eq!(json["settings"]["auto_download_new_favorites"], true);
    assert!(json["status"]["next_run_at"].as_str().is_some());
}

#[tokio::test]
async fn scheduled_sync_run_now_uses_settings_endpoint_and_requires_qobuz() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/settings/qobuz-scheduled-sync/run")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn scheduled_sync_run_now_auto_downloads_new_favorites_when_enabled() {
    let mock = MockQobuz::with_albums(vec![MockQobuz::album(77, "New Album", "Artist A")]);
    let state = state_with_mock(mock).await;
    let app = app::app(state);

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/qobuz-scheduled-sync")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"auto_download_new_favorites":true,"cron_expression":"0 3 * * *"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let run = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/settings/qobuz-scheduled-sync/run")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let run_status = run.status();
    let run_body = run.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        run_status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&run_body)
    );

    let downloads = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/downloads")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&downloads.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
    assert_eq!(json["items"][0]["qobuz_id"], 77);
}

#[tokio::test]
async fn scheduled_sync_run_now_auto_downloads_existing_unsynced_favorites_when_enabled() {
    let mock = MockQobuz::with_albums(vec![MockQobuz::album(79, "Existing Album", "Artist C")]);
    let state = state_with_mock(mock).await;
    let app = app::app(state);

    let initial_sync = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(initial_sync.status(), StatusCode::OK);

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/qobuz-scheduled-sync")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"auto_download_new_favorites":true,"cron_expression":"0 3 * * *"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let run = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/settings/qobuz-scheduled-sync/run")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let run_status = run.status();
    let run_body = run.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        run_status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&run_body)
    );

    let downloads = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/downloads")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&downloads.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
    assert_eq!(json["items"][0]["qobuz_id"], 79);
}

#[tokio::test]
async fn manual_qobuz_sync_stays_list_refresh_only_when_auto_download_enabled() {
    let mock = MockQobuz::with_albums(vec![MockQobuz::album(88, "Manual Album", "Artist B")]);
    let state = state_with_mock(mock).await;
    let app = app::app(state);

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/qobuz-scheduled-sync")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"auto_download_new_favorites":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let sync = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sync.status(), StatusCode::OK);

    let downloads = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/downloads")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&downloads.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn sync_with_mock_populates_db() {
    let mock = MockQobuz::with_albums(vec![
        MockQobuz::album(10, "Alpha", "Artist A"),
        MockQobuz::album(20, "Beta", "Artist B"),
    ]);
    let state = state_with_mock(mock).await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let spec = schema::load_spec();
    schema::validate_schema(&schema::schema_from_spec(&spec, "QobuzSyncResponse"), &json);
    assert_eq!(json["albums_total"], 2);
    assert_eq!(json["added"], 2);
}

#[tokio::test]
async fn list_favorites_keyset() {
    let mock = MockQobuz::with_albums(vec![
        MockQobuz::album(1, "One", "A"),
        MockQobuz::album(2, "Two", "B"),
        MockQobuz::album(3, "Three", "C"),
    ]);
    let state = state_with_mock(mock).await;
    let app = app::app(state.clone());

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/favorites?type=album&limit=2&sort=title&order=asc&library_filter=all")
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
        &schema::schema_from_spec(&spec, "QobuzFavoritesListResponse"),
        &json,
    );
    assert_eq!(json["items"].as_array().unwrap().len(), 2);
    assert_eq!(json["has_more"], true);
    let cursor = json["next_cursor"].as_str().unwrap();

    let mismatched_cursor = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/qobuz/favorites?type=album&limit=2&sort=title&order=asc&cursor={cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mismatched_cursor.status(), StatusCode::BAD_REQUEST);

    let page2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/qobuz/favorites?type=album&limit=2&sort=title&order=asc&library_filter=all&cursor={cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let p2: serde_json::Value =
        serde_json::from_slice(&page2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(p2["items"].as_array().unwrap().len(), 1);

    let filtered = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/favorites?type=album&q=Two&sort=artist&library_filter=all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let f: serde_json::Value =
        serde_json::from_slice(&filtered.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(f["items"].as_array().unwrap().len(), 1);
    assert_eq!(f["items"][0]["title"], "Two");

    let bad_sort = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/favorites?type=album&sort=invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_sort.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_favorites_defaults_to_in_library_with_explicit_filter_modes() {
    let mock = MockQobuz::with_albums(vec![
        MockQobuz::album(1, "Local Favorite", "A"),
        MockQobuz::album(2, "Remote Favorite", "B"),
    ]);
    let state = state_with_mock(mock).await;
    let app = app::app(state.clone());

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let artist_id = catalog::upsert_artist_by_name(&state.data, "A", None)
        .await
        .unwrap();
    catalog::upsert_album(
        &state.data,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Local Favorite",
            year: None,
            qobuz_album_id: Some(1),
            path: Some("A/Local Favorite"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let default_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/favorites?type=album")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(default_response.status(), StatusCode::OK);
    let default_json: serde_json::Value = serde_json::from_slice(
        &default_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(default_json["items"].as_array().unwrap().len(), 1);
    assert_eq!(default_json["items"][0]["qobuz_id"], 1);
    assert_eq!(default_json["items"][0]["in_library"], true);

    let all_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/favorites?type=album&library_filter=all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let all_json: serde_json::Value =
        serde_json::from_slice(&all_response.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(all_json["items"].as_array().unwrap().len(), 2);

    let remote_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/favorites?type=album&library_filter=not_in_library")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let remote_json: serde_json::Value = serde_json::from_slice(
        &remote_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(remote_json["items"].as_array().unwrap().len(), 1);
    assert_eq!(remote_json["items"][0]["qobuz_id"], 2);
    assert_eq!(remote_json["items"][0]["in_library"], false);
}

#[tokio::test]
async fn add_and_remove_favorites() {
    let mock = MockQobuz::with_albums(vec![]);
    let state = state_with_mock(mock).await;
    let app = app::app(state);

    let add = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/favorites")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"album_ids":[42]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(add.status(), StatusCode::NO_CONTENT);

    let del = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/qobuz/favorites")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"album_ids":[42]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn sync_second_run_marks_removed() {
    let mock = MockQobuz::with_albums(vec![
        MockQobuz::album(1, "A", "X"),
        MockQobuz::album(2, "B", "Y"),
    ]);
    let albums = mock.albums.clone();
    let state = state_with_mock(mock).await;
    let app = app::app(state);

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    *albums.lock().await = vec![MockQobuz::album(1, "A", "X")];

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["removed"], 1);
}

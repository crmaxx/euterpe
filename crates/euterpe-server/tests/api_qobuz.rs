use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

mod support;
use support::{state_with_mock, MockQobuz};

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

    let spec = support::load_spec();
    support::validate_schema(
        &support::schema_from_spec(&spec, "QobuzSyncResponse"),
        &json,
    );
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
                .uri("/api/v1/qobuz/favorites?type=album&limit=2&sort=title&order=asc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let spec = support::load_spec();
    support::validate_schema(
        &support::schema_from_spec(&spec, "QobuzFavoritesListResponse"),
        &json,
    );
    assert_eq!(json["items"].as_array().unwrap().len(), 2);
    assert_eq!(json["has_more"], true);
    let cursor = json["next_cursor"].as_str().unwrap();

    let page2 = app
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
    let p2: serde_json::Value = serde_json::from_slice(
        &page2.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(p2["items"].as_array().unwrap().len(), 1);

    let filtered = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/favorites?type=album&q=Two&sort=artist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let f: serde_json::Value = serde_json::from_slice(
        &filtered.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
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

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

mod support;

const SILENT_FLAC: &[u8] = include_bytes!("fixtures/silent.flac");

async fn setup_album_with_integration() -> (euterpe_server::AppState, i64, i64) {
    let state = support::test_state().await;
    let pool = state.db.clone();
    let library = state.config.library_path.clone();

    let album_dir = library.join("Test Artist/Test Album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let track_path = album_dir.join("01-track.flac");
    std::fs::write(&track_path, SILENT_FLAC).unwrap();

    let artist_id = euterpe_server::db::artists::upsert_by_name(&pool, "Test Artist", None)
        .await
        .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &pool,
        euterpe_server::db::albums::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Test Album",
            year: Some(2020),
            qobuz_album_id: None,
            path: Some("Test Artist/Test Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let rel = PathBuf::from("Test Artist/Test Album/01-track.flac");
    let _track_id = euterpe_server::db::tracks::upsert(
        &pool,
        euterpe_server::db::tracks::TrackUpsert {
            album_id,
            title: "Track One",
            track_number: Some(1),
            year: Some(2020),
            disc_number: Some(1),
            genre: None,
            qobuz_track_id: None,
            path: rel.to_str().unwrap(),
            duration_sec: Some(180),
            file_mtime: None,
            file_hash: None,
            file_size: None,
        },
    )
    .await
    .unwrap();

    let integration_id = euterpe_server::db::integrations::insert(
        &pool,
        euterpe_server::db::integrations::IntegrationInsert {
            type_: euterpe_server::integrations::IntegrationType::TagSource,
            provider: euterpe_server::integrations::IntegrationProvider::MusicBrainz,
            display_name: "MusicBrainz",
            enabled: true,
            config_json: r#"{"contact":"test@example.com"}"#,
            config_secrets_enc: None,
            sort_order: 0,
        },
    )
    .await
    .unwrap();

    (state, album_id, integration_id)
}

#[tokio::test]
async fn album_metadata_lookup_requires_valid_album() {
    let (state, _album_id, integration_id) = setup_album_with_integration().await;
    let app = app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/albums/99999/metadata/lookup")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "integration_id": integration_id }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

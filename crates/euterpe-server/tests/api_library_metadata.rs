use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_data::fixtures::{catalog, integrations};
use euterpe_server::app;
use serde_json::json;
use tower::ServiceExt;

const SILENT_FLAC: &[u8] = include_bytes!("fixtures/silent.flac");

async fn setup_album_with_integration() -> (euterpe_server::AppState, i64, i64) {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();

    let album_dir = library.join("Test Artist/Test Album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let track_path = album_dir.join("01-track.flac");
    std::fs::write(&track_path, SILENT_FLAC).unwrap();

    let album_id = catalog::seed_album(
        &state.data,
        catalog::AlbumFixture {
            artist: catalog::ArtistFixture {
                name: "Test Artist".to_string(),
                qobuz_artist_id: None,
            },
            title: "Test Album".to_string(),
            year: Some(2020),
            qobuz_album_id: None,
            path: Some("Test Artist/Test Album".to_string()),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let rel = PathBuf::from("Test Artist/Test Album/01-track.flac");
    let _track_id = catalog::seed_track(
        &state.data,
        catalog::TrackFixture {
            album_id,
            title: "Track One".to_string(),
            track_number: Some(1),
            year: Some(2020),
            disc_number: Some(1),
            genre: None,
            qobuz_track_id: None,
            path: rel.to_str().unwrap().to_string(),
            duration_sec: Some(180),
            file_mtime: None,
            file_hash: None,
            file_size: None,
        },
    )
    .await
    .unwrap();

    let integration_id = integrations::seed_integration(
        &state.data,
        integrations::IntegrationFixture {
            type_: "tag_source".to_string(),
            provider: "musicbrainz".to_string(),
            display_name: "MusicBrainz".to_string(),
            enabled: true,
            config_json: r#"{"contact":"test@example.com"}"#.to_string(),
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

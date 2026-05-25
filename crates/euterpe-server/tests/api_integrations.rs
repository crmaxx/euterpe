use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use euterpe_server::db::integrations::{IntegrationInsert, insert as insert_integration};
use euterpe_server::db::{albums, tracks};
use euterpe_server::integrations::{IntegrationProvider, IntegrationType};
use euterpe_server::library::tags::{self, TrackTags};
use euterpe_server::services::app_settings::{self, StorageSettings};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn integrations_catalog_lists_tag_sources() {
    let state = app::test_support::test_state().await;
    let app = app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/integrations/catalog?type=tag_source")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let items = body["items"].as_array().unwrap();
    assert!(items.len() >= 4);
    let providers: Vec<_> = items
        .iter()
        .map(|i| i["provider"].as_str().unwrap())
        .collect();
    assert!(providers.contains(&"musicbrainz"));
    assert!(providers.contains(&"discogs"));
}

#[tokio::test]
async fn create_list_delete_musicbrainz_integration() {
    let state = app::test_support::test_state().await;
    let app = app(state);

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/integrations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "provider": "musicbrainz",
                        "type": "tag_source",
                        "config": { "contact": "test@example.com" }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created = json_body(create).await;
    let id = created["item"]["id"].as_i64().unwrap();

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/integrations?type=tag_source")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = json_body(list).await;
    assert!(
        list_body["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"].as_i64() == Some(id))
    );

    let del = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/integrations/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn metadata_apply_requires_configured_library_storage_before_provider_call() {
    let state = app::test_support::test_state().await;
    app_settings::save_storage(&state.db, &StorageSettings { library: None })
        .await
        .unwrap();
    app_settings::refresh_runtime(&state.runtime, &state.db, &state.config).await;
    let integration_id = insert_integration(
        &state.db,
        IntegrationInsert {
            type_: IntegrationType::TagSource,
            provider: IntegrationProvider::Tracktype,
            display_name: "TrackType",
            enabled: true,
            config_json: "{}",
            config_secrets_enc: None,
            sort_order: 0,
        },
    )
    .await
    .unwrap();
    let app = app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/albums/1/metadata/apply")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "integration_id": integration_id,
                        "candidate_id": "would-hit-network-without-storage-preflight"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("LIBRARY_STORAGE_NOT_CONFIGURED")
    );
}

fn write_minimal_wav(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..64 {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();
}

#[tokio::test]
async fn metadata_apply_uses_settings_storage_not_config_library_path() {
    let state = app::test_support::test_state().await;
    let config_library_path = state.config.library_path.clone();
    let storage_root = tempfile::tempdir().unwrap();
    app_settings::save_storage(
        &state.db,
        &StorageSettings::local(storage_root.path().display().to_string()),
    )
    .await
    .unwrap();
    app_settings::refresh_runtime(&state.runtime, &state.db, &state.config).await;

    let track_rel = "Api Artist/Api Album/01 - Old.wav";
    let track_path = storage_root.path().join(track_rel);
    write_minimal_wav(&track_path);
    tags::write_tags(
        &track_path,
        &TrackTags {
            title: "Old".into(),
            artist: "Old Artist".into(),
            album: "Old Album".into(),
            track_number: Some(1),
            year: None,
            disc_number: None,
            track_total: None,
            disc_total: None,
            genre: None,
            duration_sec: None,
            qobuz_track_id: None,
            qobuz_album_id: None,
            label: None,
            isrc: None,
            composer: None,
        },
    )
    .unwrap();

    let album_id = albums::upsert(
        &state.db,
        albums::AlbumUpsert {
            artist_id: None,
            title: "Api Album",
            year: None,
            qobuz_album_id: None,
            path: Some("Api Artist/Api Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    tracks::upsert(
        &state.db,
        tracks::TrackUpsert {
            album_id,
            title: "Old",
            track_number: Some(1),
            year: None,
            disc_number: None,
            genre: None,
            qobuz_track_id: None,
            path: track_rel,
            duration_sec: None,
            file_mtime: None,
            file_hash: None,
            file_size: None,
        },
    )
    .await
    .unwrap();

    let mut server = mockito::Server::new_async().await;
    let release = server
        .mock("GET", "/api/release/candidate-1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "artist": "Api Artist",
                "album": "Api Album Fixed",
                "year": 2025,
                "genre": "Ambient",
                "tracks": [{ "title": "Fixed Track", "track_number": 1 }]
            })
            .to_string(),
        )
        .create_async()
        .await;
    let config_json = json!({ "api_base": server.url() }).to_string();
    let integration_id = insert_integration(
        &state.db,
        IntegrationInsert {
            type_: IntegrationType::TagSource,
            provider: IntegrationProvider::Tracktype,
            display_name: "TrackType",
            enabled: true,
            config_json: &config_json,
            config_secrets_enc: None,
            sort_order: 0,
        },
    )
    .await
    .unwrap();
    let app = app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/library/albums/{album_id}/metadata/apply"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "integration_id": integration_id,
                        "candidate_id": "candidate-1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    release.assert_async().await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["tracks_updated"], 1);
    let updated = tags::read_tags(&track_path).unwrap();
    assert_eq!(updated.title, "Fixed Track");
    assert_eq!(updated.album, "Api Album Fixed");
    assert_eq!(updated.genre.as_deref(), Some("Ambient"));
    assert!(!config_library_path.join(track_rel).is_file());
}

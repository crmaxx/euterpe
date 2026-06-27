use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use euterpe_server::library::tags::{self, TrackTags};
use euterpe_server::services::app_settings::{self, StorageSettings};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

async fn use_settings_local_storage(
    state: &euterpe_server::state::AppState,
    root: &std::path::Path,
) {
    app_settings::save_storage(
        &state.data,
        &StorageSettings::local(root.display().to_string()),
    )
    .await
    .unwrap();
    app_settings::refresh_runtime(&state.runtime, &state.data, &state.config).await;
    state.invalidate_library_storage_cache().await;
}

async fn wait_for_scan_success(app: &axum::Router, scan_id: i64, expected_files: Option<i64>) {
    for _ in 0..80 {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/library/scan/{scan_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let run: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        if run["status"] == "success" {
            if let Some(n) = expected_files {
                assert_eq!(run["files_indexed"].as_i64().unwrap(), n);
            }
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("scan {scan_id} did not succeed");
}

#[tokio::test]
async fn library_scan_indexes_files() {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    std::fs::create_dir_all(library.join("Scan Artist/Scan Album")).unwrap();
    let track = library.join("Scan Artist/Scan Album/01.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&track, spec).unwrap();
    for _ in 0..256 {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();

    let app = app::app(state);
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/scan")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let body = start.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let scan_id = json["scan_id"].as_i64().unwrap();

    wait_for_scan_success(&app, scan_id, Some(1)).await;

    let albums = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(albums.status(), StatusCode::OK);
    let bytes = albums.into_body().collect().await.unwrap().to_bytes();
    let list: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(!list["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn library_albums_keyset_sort_and_search() {
    let state = app::test_support::test_state().await;
    let artist_id =
        euterpe_data::repositories::catalog::upsert_artist_by_name(&state.data, "Zed", None)
            .await
            .unwrap();
    for (title, year) in [("Alpha", 2020), ("Beta", 2021), ("Gamma", 2019)] {
        euterpe_data::repositories::catalog::upsert_album(
            &state.data,
            euterpe_data::repositories::catalog::AlbumUpsert {
                artist_id: Some(artist_id),
                title,
                year: Some(year),
                qobuz_album_id: None,
                path: None,
                cover_path: None,
            },
        )
        .await
        .unwrap();
    }

    let app = app::app(state);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?limit=2&sort=title&order=asc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["has_more"], true);
    let cursor = body["next_cursor"].as_str().unwrap();

    let page2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/library/albums?limit=2&sort=title&order=asc&cursor={cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let p2: Value =
        serde_json::from_slice(&page2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(p2["items"][0]["title"], "Gamma");

    let search = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?q=Bet&sort=title")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let s: Value =
        serde_json::from_slice(&search.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(s["items"].as_array().unwrap().len(), 1);
    assert_eq!(s["items"][0]["title"], "Beta");

    let bad = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?sort=nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn library_scan_conflict_when_running() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/scan")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let second = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/scan")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::CONFLICT);
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
async fn library_scan_subtree_root_indexes_only_under_path() {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    write_minimal_wav(&library.join("Scan Artist/Scan Album/01.wav"));
    write_minimal_wav(&library.join("Other Artist/Other Album/99.wav"));

    let app = app::app(state);
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/scan?root=Scan%20Artist%2FScan%20Album")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let body = start.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let scan_id = json["scan_id"].as_i64().unwrap();

    for _ in 0..80 {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/library/scan/{scan_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let run: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        if run["status"] == "success" {
            assert_eq!(run["files_indexed"].as_i64().unwrap(), 1);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let albums = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?limit=20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list: Value =
        serde_json::from_slice(&albums.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let titles: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Scan Album"));
    assert!(!titles.contains(&"Other Album"));
}

#[tokio::test]
async fn library_scan_root_rejects_traversal() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/scan?root=..%2F..%2Fetc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn library_scan_cancel_sets_status_and_rejects_repeat() {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    for i in 0..40 {
        write_minimal_wav(&library.join(format!("Bulk Artist/Album {i:02}/track.wav")));
    }

    let app = app::app(state);
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/scan")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let scan_id =
        serde_json::from_slice::<Value>(&start.into_body().collect().await.unwrap().to_bytes())
            .unwrap()["scan_id"]
            .as_i64()
            .unwrap();

    let cancel = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/library/scan/{scan_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancel.status(), StatusCode::NO_CONTENT);

    let mut cancelled = false;
    for _ in 0..80 {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/library/scan/{scan_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let run: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        if run["status"] == "cancelled" {
            cancelled = true;
            break;
        }
        if run["status"] == "success" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(cancelled, "expected scan to reach cancelled status");

    let again = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/library/scan/{scan_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn library_album_cover_get_returns_file_bytes() {
    let state = app::test_support::test_state().await;
    let lib = state.config.library_path.clone();
    std::fs::create_dir_all(lib.join("CovArtist/CovAlbum")).unwrap();
    std::fs::write(lib.join("CovArtist/CovAlbum/cover.jpg"), b"cover-bytes").unwrap();

    let artist_id =
        euterpe_data::repositories::catalog::upsert_artist_by_name(&state.data, "CovArtist", None)
            .await
            .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "CovAlbum",
            year: None,
            qobuz_album_id: None,
            path: Some("CovArtist/CovAlbum"),
            cover_path: Some("CovArtist/CovAlbum/cover.jpg"),
        },
    )
    .await
    .unwrap();

    let no_cover_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "NoCoverAlbum",
            year: None,
            qobuz_album_id: None,
            path: Some("CovArtist/NoCoverAlbum"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let app = app::app(state);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{album_id}/cover"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"cover-bytes");

    let no_path = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{no_cover_id}/cover"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_path.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn library_album_cover_put_writes_file_and_updates_db() {
    let state = app::test_support::test_state().await;
    let lib = state.config.library_path.clone();
    std::fs::create_dir_all(lib.join("PutArtist/PutAlbum")).unwrap();

    let artist_id =
        euterpe_data::repositories::catalog::upsert_artist_by_name(&state.data, "PutArtist", None)
            .await
            .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "PutAlbum",
            year: None,
            qobuz_album_id: None,
            path: Some("PutArtist/PutAlbum"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let png = b"\x89PNG\r\n\x1a\n";
    let app = app::app(state.clone());
    let put = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/library/albums/{album_id}/cover"))
                .header("content-type", "image/png")
                .body(Body::from(png.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(
        &http_body_util::BodyExt::collect(put.into_body())
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(json["cover_path"], "PutArtist/PutAlbum/cover.png");

    assert!(lib.join("PutArtist/PutAlbum/cover.png").is_file());
    let row = euterpe_data::repositories::catalog::get_album_by_id(&state.data, album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.cover_path.as_deref(),
        Some("PutArtist/PutAlbum/cover.png")
    );

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{album_id}/cover"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
}

#[tokio::test]
async fn library_album_cover_put_rejects_missing_album_path() {
    let state = app::test_support::test_state().await;
    let artist_id =
        euterpe_data::repositories::catalog::upsert_artist_by_name(&state.data, "NoPath", None)
            .await
            .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Ghost",
            year: None,
            qobuz_album_id: None,
            path: None,
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let app = app::app(state);
    let res = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/library/albums/{album_id}/cover"))
                .header("content-type", "image/jpeg")
                .body(Body::from(vec![0xff, 0xd8, 0xff]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn library_album_cover_put_rejects_unsupported_content_type() {
    let state = app::test_support::test_state().await;
    let lib = state.config.library_path.clone();
    std::fs::create_dir_all(lib.join("TxtArtist/TxtAlbum")).unwrap();
    let artist_id =
        euterpe_data::repositories::catalog::upsert_artist_by_name(&state.data, "TxtArtist", None)
            .await
            .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "TxtAlbum",
            year: None,
            qobuz_album_id: None,
            path: Some("TxtArtist/TxtAlbum"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let app = app::app(state);
    let res = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/library/albums/{album_id}/cover"))
                .header("content-type", "text/plain")
                .body(Body::from(b"hello".to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn library_album_cover_put_rejects_oversized_body() {
    let state = app::test_support::test_state().await;
    let lib = state.config.library_path.clone();
    std::fs::create_dir_all(lib.join("BigArtist/BigAlbum")).unwrap();
    let artist_id =
        euterpe_data::repositories::catalog::upsert_artist_by_name(&state.data, "BigArtist", None)
            .await
            .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "BigAlbum",
            year: None,
            qobuz_album_id: None,
            path: Some("BigArtist/BigAlbum"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let oversized = vec![0u8; euterpe_server::library::covers::MAX_ALBUM_COVER_BYTES + 1];
    let app = app::app(state);
    let res = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/library/albums/{album_id}/cover"))
                .header("content-type", "image/jpeg")
                .body(Body::from(oversized))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn library_patch_album_tags_updates_all_track_files() {
    use euterpe_server::library::tags::{self, TrackTags};

    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    let dir = library.join("Tag Artist/Tag Album");
    std::fs::create_dir_all(&dir).unwrap();

    fn write_wav_with_tags(path: &std::path::Path, title: &str, track_number: u32) {
        write_minimal_wav(path);
        let tags = TrackTags {
            title: title.into(),
            artist: "Old Artist".into(),
            album: "Old Album".into(),
            track_number: Some(track_number),
            year: Some(2000),
            disc_number: Some(1),
            track_total: None,
            disc_total: None,
            genre: None,
            duration_sec: None,
            qobuz_track_id: None,
            qobuz_album_id: None,
            label: None,
            isrc: None,
            composer: None,
        };
        tags::write_tags(path, &tags).unwrap();
    }

    write_wav_with_tags(&dir.join("01 One.wav"), "One", 1);
    write_wav_with_tags(&dir.join("02 Two.wav"), "Two", 2);

    let app = app::app(state);
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/scan?root=Tag%20Artist%2FTag%20Album")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let scan_id: i64 =
        serde_json::from_slice::<Value>(&start.into_body().collect().await.unwrap().to_bytes())
            .unwrap()["scan_id"]
            .as_i64()
            .unwrap();

    for _ in 0..80 {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/library/scan/{scan_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let run: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        if run["status"] == "success" {
            assert_eq!(run["files_indexed"].as_i64().unwrap(), 2);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let albums = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?limit=50&q=Old%20Album")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list: Value =
        serde_json::from_slice(&albums.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let album_id = list["items"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|a| a["id"].as_i64())
        .expect("indexed album");

    let patch_body = serde_json::json!({
        "artist_name": "New Artist",
        "album_title": "New Album",
        "year": 2024,
        "genre": "Jazz",
        "track_total": 12,
        "disc_total": 2
    });
    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/library/albums/{album_id}"))
                .header("content-type", "application/json")
                .body(Body::from(patch_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
    let detail: Value =
        serde_json::from_slice(&patch.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(detail["artist_name"], "New Artist");
    assert_eq!(detail["title"], "New Album");
    assert_eq!(detail["track_total"], 12);
    assert_eq!(detail["disc_total"], 2);

    for (file, title, num) in [("01 One.wav", "One", 1u32), ("02 Two.wav", "Two", 2u32)] {
        let read = tags::read_tags(&dir.join(file)).unwrap();
        assert_eq!(read.title, title);
        assert_eq!(read.track_number, Some(num));
        assert_eq!(read.disc_number, Some(1));
        assert_eq!(read.artist, "New Artist");
        assert_eq!(read.album, "New Album");
        assert_eq!(read.year, Some(2024));
        assert_eq!(read.genre.as_deref(), Some("Jazz"));
        assert_eq!(read.track_total, Some(12));
        assert_eq!(read.disc_total, Some(2));
    }
}

#[tokio::test]
async fn library_patch_track_tags_updates_storage_file() {
    use euterpe_server::library::tags::{self, TrackTags};

    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    let dir = library.join("TrackTag Artist/TrackTag Album");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("01 Original.wav");
    write_minimal_wav(&path);
    tags::write_tags(
        &path,
        &TrackTags {
            title: "Original".into(),
            artist: "Old Artist".into(),
            album: "Old Album".into(),
            track_number: Some(1),
            year: Some(2001),
            disc_number: Some(1),
            track_total: None,
            disc_total: None,
            genre: Some("Rock".into()),
            duration_sec: None,
            qobuz_track_id: None,
            qobuz_album_id: None,
            label: None,
            isrc: None,
            composer: None,
        },
    )
    .unwrap();

    let artist_id = euterpe_data::repositories::catalog::upsert_artist_by_name(
        &state.data,
        "TrackTag Artist",
        None,
    )
    .await
    .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "TrackTag Album",
            year: Some(2001),
            qobuz_album_id: None,
            path: Some("TrackTag Artist/TrackTag Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let track_id = euterpe_data::repositories::catalog::upsert_track(
        &state.data,
        euterpe_data::repositories::catalog::TrackUpsert {
            album_id,
            title: "Original",
            track_number: Some(1),
            year: Some(2001),
            disc_number: Some(1),
            genre: Some("Rock"),
            qobuz_track_id: None,
            path: "TrackTag Artist/TrackTag Album/01 Original.wav",
            duration_sec: Some(1),
            file_mtime: None,
            file_hash: None,
            file_size: None,
        },
    )
    .await
    .unwrap();

    let app = app::app(state);
    let patch_body = serde_json::json!({
        "title": "Patched",
        "artist_name": "New Artist",
        "album_title": "New Album",
        "track_number": 7,
        "year": 2026,
        "disc_number": 2,
        "genre": "Jazz"
    });
    let patch = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/library/tracks/{track_id}"))
                .header("content-type", "application/json")
                .body(Body::from(patch_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
    let detail: Value =
        serde_json::from_slice(&patch.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(detail["title"], "Patched");
    assert_eq!(detail["artist_name"], "New Artist");
    assert_eq!(detail["album_title"], "New Album");

    let read = tags::read_tags(&path).unwrap();
    assert_eq!(read.title, "Patched");
    assert_eq!(read.artist, "New Artist");
    assert_eq!(read.album, "New Album");
    assert_eq!(read.track_number, Some(7));
    assert_eq!(read.year, Some(2026));
    assert_eq!(read.disc_number, Some(2));
    assert_eq!(read.genre.as_deref(), Some("Jazz"));
}

#[tokio::test]
async fn library_track_stream_serves_audio() {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    let path = library.join("Stream Artist/Stream Album/play.wav");
    write_minimal_wav(&path);

    let artist_id = euterpe_data::repositories::catalog::upsert_artist_by_name(
        &state.data,
        "Stream Artist",
        None,
    )
    .await
    .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Stream Album",
            year: Some(2020),
            qobuz_album_id: None,
            path: Some("Stream Artist/Stream Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let track_id = euterpe_data::repositories::catalog::upsert_track(
        &state.data,
        euterpe_data::repositories::catalog::TrackUpsert {
            album_id,
            title: "Play",
            path: "Stream Artist/Stream Album/play.wav",
            track_number: Some(1),
            year: Some(2020),
            disc_number: Some(1),
            genre: None,
            qobuz_track_id: None,
            duration_sec: Some(1),
            file_mtime: None,
            file_hash: None,
            file_size: None,
        },
    )
    .await
    .unwrap();

    let app = app::app(state);
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/tracks/{track_id}/stream"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(ct, "audio/wav");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert!(!bytes.is_empty());
}

fn write_wav_with_byte_length(path: &std::path::Path, target_len: u64) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    while std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) < target_len {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();
}

#[tokio::test]
async fn library_track_stream_range_returns_partial_content() {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    let path = library.join("Range Artist/Range Album/range.wav");
    write_wav_with_byte_length(&path, 2048);

    let artist_id = euterpe_data::repositories::catalog::upsert_artist_by_name(
        &state.data,
        "Range Artist",
        None,
    )
    .await
    .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Range Album",
            year: Some(2020),
            qobuz_album_id: None,
            path: Some("Range Artist/Range Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let track_id = euterpe_data::repositories::catalog::upsert_track(
        &state.data,
        euterpe_data::repositories::catalog::TrackUpsert {
            album_id,
            title: "Range",
            path: "Range Artist/Range Album/range.wav",
            track_number: Some(1),
            year: Some(2020),
            disc_number: Some(1),
            genre: None,
            qobuz_track_id: None,
            duration_sec: Some(1),
            file_mtime: None,
            file_hash: None,
            file_size: None,
        },
    )
    .await
    .unwrap();

    let full_len = tokio::fs::metadata(&path).await.unwrap().len();

    let app = app::app(state);
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/tracks/{track_id}/stream"))
                .header("Range", "bytes=0-1023")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    let cr = res
        .headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(cr, format!("bytes 0-1023/{full_len}"));
    let cl = res
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(cl, "1024");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.len(), 1024);
}

#[tokio::test]
async fn library_scan_uses_settings_storage_root() {
    let state = app::test_support::test_state().await;
    let storage_root = tempfile::tempdir().unwrap();
    use_settings_local_storage(&state, storage_root.path()).await;

    write_minimal_wav(
        &storage_root
            .path()
            .join("Settings Artist/Settings Album/01.wav"),
    );

    let app = app::app(state);
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/library/scan")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let scan_id: i64 =
        serde_json::from_slice::<Value>(&start.into_body().collect().await.unwrap().to_bytes())
            .unwrap()["scan_id"]
            .as_i64()
            .unwrap();

    wait_for_scan_success(&app, scan_id, Some(1)).await;

    let albums = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?limit=20&q=Settings%20Album")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(albums.status(), StatusCode::OK);
    let list: Value =
        serde_json::from_slice(&albums.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["title"], "Settings Album");
}

#[tokio::test]
async fn library_track_detail_reads_tags_via_settings_storage() {
    let state = app::test_support::test_state().await;
    let storage_root = tempfile::tempdir().unwrap();
    use_settings_local_storage(&state, storage_root.path()).await;

    let track_rel = "TagRead Artist/TagRead Album/01 Read.wav";
    let track_path = storage_root.path().join(track_rel);
    write_minimal_wav(&track_path);
    tags::write_tags(
        &track_path,
        &TrackTags {
            title: "Read Title".into(),
            artist: "TagRead Artist".into(),
            album: "TagRead Album".into(),
            track_number: Some(3),
            year: Some(1999),
            disc_number: Some(2),
            track_total: None,
            disc_total: None,
            genre: Some("Folk".into()),
            duration_sec: None,
            qobuz_track_id: None,
            qobuz_album_id: None,
            label: None,
            isrc: None,
            composer: None,
        },
    )
    .unwrap();

    let artist_id = euterpe_data::repositories::catalog::upsert_artist_by_name(
        &state.data,
        "TagRead Artist",
        None,
    )
    .await
    .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "TagRead Album",
            year: Some(1999),
            qobuz_album_id: None,
            path: Some("TagRead Artist/TagRead Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let track_id = euterpe_data::repositories::catalog::upsert_track(
        &state.data,
        euterpe_data::repositories::catalog::TrackUpsert {
            album_id,
            title: "Read Title",
            track_number: Some(3),
            year: Some(1999),
            disc_number: Some(2),
            genre: Some("Folk"),
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

    let app = app::app(state);
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/tracks/{track_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let detail: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(detail["title"], "Read Title");
    assert_eq!(detail["artist_name"], "TagRead Artist");
    assert_eq!(detail["album_title"], "TagRead Album");
    assert_eq!(detail["track_number"], 3);
    assert_eq!(detail["year"], 1999);
    assert_eq!(detail["genre"], "Folk");
}

#[tokio::test]
async fn library_album_list_discovers_cover_via_settings_storage() {
    let state = app::test_support::test_state().await;
    let storage_root = tempfile::tempdir().unwrap();
    use_settings_local_storage(&state, storage_root.path()).await;

    let album_rel = "CoverDisc Artist/CoverDisc Album";
    std::fs::create_dir_all(storage_root.path().join(album_rel)).unwrap();
    std::fs::write(
        storage_root.path().join(format!("{album_rel}/cover.jpg")),
        b"settings-cover",
    )
    .unwrap();

    let artist_id = euterpe_data::repositories::catalog::upsert_artist_by_name(
        &state.data,
        "CoverDisc Artist",
        None,
    )
    .await
    .unwrap();
    let album_id = euterpe_data::repositories::catalog::upsert_album(
        &state.data,
        euterpe_data::repositories::catalog::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "CoverDisc Album",
            year: None,
            qobuz_album_id: None,
            path: Some(album_rel),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let app = app::app(state);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?limit=20&q=CoverDisc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let item = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"].as_i64() == Some(album_id))
        .expect("album in list");
    assert_eq!(
        item["cover_path"].as_str(),
        Some("CoverDisc Artist/CoverDisc Album/cover.jpg")
    );

    let cover = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{album_id}/cover"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cover.status(), StatusCode::OK);
    let body = cover.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"settings-cover");
}

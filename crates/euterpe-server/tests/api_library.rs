use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

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

    for _ in 0..50 {
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
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let run: Value = serde_json::from_slice(&bytes).unwrap();
        if run["status"] == "success" {
            assert!(run["files_indexed"].as_i64().unwrap() >= 1);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

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
        euterpe_server::db::artists::upsert_by_name(&state.db, "Zed", None)
            .await
            .unwrap();
    for (title, year) in [("Alpha", 2020), ("Beta", 2021), ("Gamma", 2019)] {
        euterpe_server::db::albums::upsert(
            &state.db,
            euterpe_server::db::albums::AlbumUpsert {
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
    let body: Value = serde_json::from_slice(
        &res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
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
    let p2: Value = serde_json::from_slice(
        &page2.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
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
    let s: Value = serde_json::from_slice(
        &search.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
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
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
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
    let list: Value = serde_json::from_slice(
        &albums.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
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
        write_minimal_wav(
            &library.join(format!("Bulk Artist/Album {i:02}/track.wav")),
        );
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
    let scan_id = serde_json::from_slice::<Value>(
        &start.into_body().collect().await.unwrap().to_bytes(),
    )
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
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
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
        euterpe_server::db::artists::upsert_by_name(&state.db, "CovArtist", None)
            .await
            .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
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

    let no_cover_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
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
        euterpe_server::db::artists::upsert_by_name(&state.db, "PutArtist", None)
            .await
            .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
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
    let json: serde_json::Value =
        serde_json::from_slice(&http_body_util::BodyExt::collect(put.into_body()).await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["cover_path"], "PutArtist/PutAlbum/cover.png");

    assert!(lib.join("PutArtist/PutAlbum/cover.png").is_file());
    let row = euterpe_server::db::albums::get_by_id(&state.db, album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.cover_path.as_deref(), Some("PutArtist/PutAlbum/cover.png"));

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
        euterpe_server::db::artists::upsert_by_name(&state.db, "NoPath", None)
            .await
            .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
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
        euterpe_server::db::artists::upsert_by_name(&state.db, "TxtArtist", None)
            .await
            .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
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
        euterpe_server::db::artists::upsert_by_name(&state.db, "BigArtist", None)
            .await
            .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
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
    let scan_id: i64 = serde_json::from_slice::<Value>(
        &start.into_body().collect().await.unwrap().to_bytes(),
    )
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
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
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
    let list: Value = serde_json::from_slice(
        &albums.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
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
    let detail: Value = serde_json::from_slice(
        &patch.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(detail["artist_name"], "New Artist");
    assert_eq!(detail["title"], "New Album");
    assert_eq!(detail["track_total"], 12);
    assert_eq!(detail["disc_total"], 2);

    for (file, title, num) in [
        ("01 One.wav", "One", 1u32),
        ("02 Two.wav", "Two", 2u32),
    ] {
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
async fn library_track_stream_serves_audio() {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    let path = library.join("Stream Artist/Stream Album/play.wav");
    write_minimal_wav(&path);

    let artist_id =
        euterpe_server::db::artists::upsert_by_name(&state.db, "Stream Artist", None)
            .await
            .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
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
    let track_id = euterpe_server::db::tracks::upsert(
        &state.db,
        euterpe_server::db::tracks::TrackUpsert {
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

    let artist_id =
        euterpe_server::db::artists::upsert_by_name(&state.db, "Range Artist", None)
            .await
            .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
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
    let track_id = euterpe_server::db::tracks::upsert(
        &state.db,
        euterpe_server::db::tracks::TrackUpsert {
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

mod support;

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
    assert!(list["total"].as_i64().unwrap() >= 1);
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

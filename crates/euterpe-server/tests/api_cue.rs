use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use euterpe_server::app;

fn write_test_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for sample in 0..(44_100 * 2) {
        writer.write_sample((sample % 1024) as i16).unwrap();
    }
    writer.finalize().unwrap();
}

fn make_flac_image(dir: &std::path::Path) -> std::path::PathBuf {
    let wav = dir.join("album.wav");
    write_test_wav(&wav);
    euterpe_converter::convert_file(
        &wav,
        euterpe_converter::ConvertOptions {
            flac_encode: &euterpe_converter::FlacEncodeSettings::default(),
            file_policy: euterpe_converter::FilePolicy::SiblingThenDelete,
            on_progress: None,
        },
    )
    .unwrap()
    .output_path
}

async fn seed_album_with_cue() -> (euterpe_server::AppState, i64) {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    let album_rel = "Cue Artist/Cue Album";
    let album_dir = library.join(album_rel);
    std::fs::create_dir_all(&album_dir).unwrap();
    std::fs::write(album_dir.join("album.flac"), b"not-a-real-flac").unwrap();
    std::fs::write(
        album_dir.join("album.cue"),
        r#"
REM GENRE "Dance"
REM DATE 2007
REM COMMENT "Vinyl rip"
PERFORMER "Cue Artist"
TITLE "Cue Album"
FILE "album.flac" FLAC
  TRACK 01 AUDIO
    TITLE "One"
    PERFORMER "Track Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Two"
    INDEX 01 00:01:00
"#,
    )
    .unwrap();
    let artist_id = euterpe_server::db::artists::upsert_by_name(&state.db, "Cue Artist", None)
        .await
        .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Cue Album",
            year: Some(2007),
            qobuz_album_id: None,
            path: Some(album_rel),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    (state, album_id)
}

async fn seed_album_with_real_flac_cue() -> (euterpe_server::AppState, i64, std::path::PathBuf) {
    let state = app::test_support::test_state().await;
    let library = state.config.library_path.clone();
    let album_rel = "Cue Artist/Real Album";
    let album_dir = library.join(album_rel);
    std::fs::create_dir_all(&album_dir).unwrap();
    make_flac_image(&album_dir);
    std::fs::write(
        album_dir.join("album.cue"),
        r#"
REM GENRE "Rock"
REM DATE 1972
PERFORMER "Cue Artist"
TITLE "Real Album"
FILE "album.flac" FLAC
  TRACK 01 AUDIO
    TITLE "One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Two"
    INDEX 01 00:01:00
"#,
    )
    .unwrap();
    let artist_id = euterpe_server::db::artists::upsert_by_name(&state.db, "Cue Artist", None)
        .await
        .unwrap();
    let album_id = euterpe_server::db::albums::upsert(
        &state.db,
        euterpe_server::db::albums::AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Real Album",
            year: Some(1972),
            qobuz_album_id: None,
            path: Some(album_rel),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    (state, album_id, album_dir)
}

#[tokio::test]
async fn album_detail_and_list_expose_cue_availability() {
    let (state, album_id) = seed_album_with_cue().await;
    let app = app::app(state);

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{album_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&detail.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["has_cue_files"], true);

    let list = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/library/albums?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list_body: Value =
        serde_json::from_slice(&list.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(list_body["items"][0]["has_cue_files"], true);
}

#[tokio::test]
async fn get_album_cue_returns_editable_document_and_choices() {
    let (state, album_id) = seed_album_with_cue().await;
    let app = app::app(state);

    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{album_id}/cue"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        body["cue_files"][0]["path"],
        "Cue Artist/Cue Album/album.cue"
    );
    assert_eq!(body["document"]["album_artist"], "Cue Artist");
    assert_eq!(body["document"]["album_title"], "Cue Album");
    assert_eq!(body["document"]["year"], 2007);
    assert_eq!(body["document"]["genre"], "Dance");
    assert_eq!(body["document"]["tracks"].as_array().unwrap().len(), 2);
    assert_eq!(body["validation"]["valid"], true);
}

#[tokio::test]
async fn validate_album_cue_reports_missing_required_fields() {
    let (state, album_id) = seed_album_with_cue().await;
    let app = app::app(state);
    let body = json!({
        "document": {
            "cue_path": "Cue Artist/Cue Album/album.cue",
            "audio_path": "album.flac",
            "audio_format": "flac",
            "album_title": "",
            "album_artist": "Cue Artist",
            "year": null,
            "genre": null,
            "comment": null,
            "extra_fields": [],
            "tracks": [{
                "number": 1,
                "artist": null,
                "title": "",
                "genre": null,
                "start_index": "00:00:00",
                "pregap": null,
                "duration": null,
                "selected": true
            }]
        }
    });

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/library/albums/{album_id}/cue/validate"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let json: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["valid"], false);
    let codes: Vec<&str> = json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"missing_album_title"));
    assert!(codes.contains(&"missing_album_year"));
    assert!(codes.contains(&"missing_album_genre"));
    assert!(codes.contains(&"missing_track_title"));
}

#[tokio::test]
async fn split_album_cue_creates_latest_job() {
    let (state, album_id) = seed_album_with_cue().await;
    let app = app::app(state);
    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{album_id}/cue"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cue_body: Value =
        serde_json::from_slice(&get.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let split_body = json!({
        "document": cue_body["document"],
        "source_file_policy": "keep",
        "file_mask": null
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/library/albums/{album_id}/cue/split"))
                .header("content-type", "application/json")
                .body(Body::from(split_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let latest = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{album_id}/cue/latest"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let latest_body: Value =
        serde_json::from_slice(&latest.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(latest_body["job"]["album_id"], album_id);
}

#[tokio::test]
async fn split_album_cue_delete_after_success_removes_image_and_cue() {
    let (state, album_id, album_dir) = seed_album_with_real_flac_cue().await;
    let app = app::app(state);
    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/library/albums/{album_id}/cue"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cue_body: Value =
        serde_json::from_slice(&get.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let split_body = json!({
        "document": cue_body["document"],
        "source_file_policy": "delete_after_success",
        "file_mask": null
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/library/albums/{album_id}/cue/split"))
                .header("content-type", "application/json")
                .body(Body::from(split_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    let mut status = String::new();
    for _ in 0..50 {
        let latest = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/library/albums/{album_id}/cue/latest"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let latest_body: Value =
            serde_json::from_slice(&latest.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        status = latest_body["job"]["status"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if status == "success" || status == "failed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    assert_eq!(status, "success");
    assert!(!album_dir.join("album.flac").exists());
    assert!(!album_dir.join("album.cue").exists());
}

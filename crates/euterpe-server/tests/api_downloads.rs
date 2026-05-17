mod support;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::{routing::get, Router};
use euterpe_qobuz::{AlbumDetail, AlbumSummary, ArtistRef, TrackSummary};
use euterpe_qobuz::AlbumTracks;
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

use support::{
    load_spec, schema_from_spec, state_with_download_mock, validate_schema, DownloadMockQobuz,
};

#[tokio::test]
async fn create_download_returns_202() {
    let mock = DownloadMockQobuz::new();
    let state = state_with_download_mock(mock).await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/downloads")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"job_type":"album","qobuz_id":99,"quality":6}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let spec = load_spec();
    validate_schema(
        &schema_from_spec(&spec, "CreateDownloadResponse"),
        &json,
    );
}

#[tokio::test]
async fn download_job_completes_via_worker() {
    let body = b"audio-bytes";
    let cdn = Router::new().route("/cdn", get(|| async { body.to_vec() }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, cdn).await.unwrap() });

    let album = AlbumDetail {
        summary: AlbumSummary {
            id: 99,
            title: "Test".into(),
            artist: Some(ArtistRef {
                id: 1,
                name: "Artist".into(),
            }),
            artists: None,
            image: None,
            release_date_original: None,
            hires: None,
        },
        tracks: Some(AlbumTracks {
            items: vec![TrackSummary {
                id: 1,
                title: "Track".into(),
                track_number: Some(1),
                duration: None,
                performer: None,
                hires_streamable: None,
            }],
        }),
        description: None,
    };

    let mock = DownloadMockQobuz {
        album,
        stream_url: format!("http://{addr}/cdn"),
    };
    let state = state_with_download_mock(mock).await;
    let app = app::app(state);

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/downloads")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"job_type":"album","qobuz_id":99,"quality":6}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::ACCEPTED);
    let created: serde_json::Value =
        serde_json::from_slice(&create.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let job_id = created["job_id"].as_i64().unwrap();

    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/downloads/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if get.status() == StatusCode::OK {
            let job: serde_json::Value = serde_json::from_slice(
                &get.into_body().collect().await.unwrap().to_bytes(),
            )
            .unwrap();
            if job["status"] == "completed" {
                return;
            }
        }
    }
    panic!("job did not complete in time");
}

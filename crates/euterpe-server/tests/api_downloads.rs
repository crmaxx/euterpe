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

use euterpe_server::api::DownloadJobType;
use euterpe_server::db::download_jobs;
use euterpe_server::services::download::DownloadJobPayload;

use support::{
    load_spec, schema_from_spec, state_with_download_mock, test_state, validate_schema,
    DownloadMockQobuz,
};

#[tokio::test]
async fn create_download_rejects_empty_album_api_id() {
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
                    r#"{"job_type":"album","album_api_id":"","quality":6}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

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
                    r#"{"job_type":"album","album_api_id":"99","qobuz_id":99,"quality":6}"#,
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
async fn create_download_by_url_returns_202() {
    let mock = DownloadMockQobuz::new();
    let state = state_with_download_mock(mock).await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/downloads/by-url")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"url":"https://play.qobuz.com/album/zg7pv28g4mldg","quality":6}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("job_id").is_some());
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
            qobuz_id: None,
            title: "Test".into(),
            artist: Some(ArtistRef {
                id: 1,
                name: "Artist".into(),
            }),
            artists: None,
            image: None,
            release_date_original: None,
            hires: None,
            album_ref: None,
            slug: None,
            list_id: None,
            product_id: None,
            genre: None,
            label: None,
        },
        tracks: Some(AlbumTracks {
            items: vec![TrackSummary {
                id: 1,
                title: "Track".into(),
                track_number: Some(1),
                duration: None,
                performer: None,
                hires_streamable: None,
                media_number: None,
                genre: None,
                isrc: None,
                composer: None,
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
                    r#"{"job_type":"album","album_api_id":"99","qobuz_id":99,"quality":6}"#,
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

#[tokio::test]
async fn purge_finished_deletes_terminal_jobs() {
    let state = test_state().await;
    let pool = state.db.clone();
    let payload = DownloadJobPayload {
        album_api_id: Some("1".into()),
    };

    let queued = download_jobs::insert_queued(&pool, DownloadJobType::Album, 1, 6, Some(&payload))
        .await
        .unwrap();
    let running = download_jobs::insert_queued(&pool, DownloadJobType::Album, 2, 6, Some(&payload))
        .await
        .unwrap();
    download_jobs::claim_running(&pool, running).await.unwrap();
    let done = download_jobs::insert_queued(&pool, DownloadJobType::Album, 3, 6, Some(&payload))
        .await
        .unwrap();
    download_jobs::claim_running(&pool, done).await.unwrap();
    download_jobs::finish_success(&pool, done).await.unwrap();
    let failed = download_jobs::insert_queued(&pool, DownloadJobType::Album, 4, 6, Some(&payload))
        .await
        .unwrap();
    download_jobs::finish_failed(&pool, failed, "err").await.unwrap();

    let app = app::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/downloads/purge")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["deleted"], 2);
    let spec = load_spec();
    validate_schema(
        &schema_from_spec(&spec, "DownloadPurgeResponse"),
        &json,
    );

    assert!(download_jobs::get(&pool, queued).await.unwrap().is_some());
    assert!(download_jobs::get(&pool, running).await.unwrap().is_some());
    assert!(download_jobs::get(&pool, done).await.unwrap().is_none());
    assert!(download_jobs::get(&pool, failed).await.unwrap().is_none());
}

#[tokio::test]
async fn delete_with_purge_removes_terminal_job() {
    let state = test_state().await;
    let pool = state.db.clone();
    let payload = DownloadJobPayload {
        album_api_id: Some("1".into()),
    };
    let id = download_jobs::insert_queued(&pool, DownloadJobType::Album, 1, 6, Some(&payload))
        .await
        .unwrap();
    download_jobs::claim_running(&pool, id).await.unwrap();
    download_jobs::finish_success(&pool, id).await.unwrap();

    let app = app::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/downloads/{id}?purge=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(download_jobs::get(&pool, id).await.unwrap().is_none());
}

#[tokio::test]
async fn delete_with_purge_rejects_running_job() {
    let state = test_state().await;
    let pool = state.db.clone();
    let payload = DownloadJobPayload {
        album_api_id: Some("1".into()),
    };
    let id = download_jobs::insert_queued(&pool, DownloadJobType::Album, 1, 6, Some(&payload))
        .await
        .unwrap();
    download_jobs::claim_running(&pool, id).await.unwrap();

    let app = app::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/downloads/{id}?purge=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(download_jobs::get(&pool, id).await.unwrap().is_some());
}

#[tokio::test]
async fn list_downloads_keyset_by_id_desc() {
    let state = test_state().await;
    let pool = state.db.clone();
    for i in 1..=4 {
        let payload = DownloadJobPayload {
            album_api_id: Some(format!("album-{i}")),
        };
        download_jobs::insert_queued(&pool, DownloadJobType::Album, i, 6, Some(&payload))
            .await
            .unwrap();
    }

    let app = app::app(state);
    let page1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/downloads?limit=2&sort=id&order=desc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page1.status(), StatusCode::OK);
    let b1: serde_json::Value = serde_json::from_slice(
        &page1.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let spec = load_spec();
    validate_schema(
        &schema_from_spec(&spec, "DownloadJobListResponse"),
        &b1,
    );
    assert_eq!(b1["items"].as_array().unwrap().len(), 2);
    assert_eq!(b1["has_more"], true);
    let cursor = b1["next_cursor"].as_str().unwrap();

    let page2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/downloads?limit=2&sort=id&order=desc&cursor={cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page2.status(), StatusCode::OK);
    let b2: serde_json::Value = serde_json::from_slice(
        &page2.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(b2["items"].as_array().unwrap().len(), 2);
    assert_eq!(b2["has_more"], false);
}

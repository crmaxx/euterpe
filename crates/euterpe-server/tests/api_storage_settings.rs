use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn storage_settings_encrypts_smb_password_and_redacts_response() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/storage")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "library": {
                            "kind": "smb",
                            "host": "192.168.0.124",
                            "port": 445,
                            "share": "dietpi",
                            "path": "Musik",
                            "username": "roon",
                            "password": "secret",
                            "workgroup": "WORKGROUP"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["kind"], "smb");
    assert_eq!(json["settings"]["library"]["host"], "192.168.0.124");
    assert!(json["settings"]["library"].get("password").is_none());
    assert!(
        json["settings"]["library"]
            .get("password_encrypted")
            .is_none()
    );

    let raw: (String,) =
        sqlx::query_as("SELECT value FROM settings WHERE key = 'storage.settings'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert!(!raw.0.contains("secret"));
    assert!(raw.0.contains("password_encrypted"));
}

#[tokio::test]
async fn storage_settings_rejects_smb_password_without_master_key() {
    let config = euterpe_server::AppConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        database_url: "sqlite::memory:".into(),
        admin_password: None,
        master_key: None,
        public_base_url: "http://127.0.0.1:0".into(),
        oauth_state_ttl: std::time::Duration::from_secs(600),
        qobuz_api_base: None,
        qobuz_play_base: None,
        library_path: std::env::temp_dir().join("euterpe-storage-test"),
        torrent_incoming_dir: None,
        torrent_max_active: 2,
        torrent_enable_upnp: false,
        download_concurrency: 2,
        library_scan: euterpe_server::config::LibraryScanConfig::default(),
        debug: false,
        static_dir: std::path::PathBuf::new(),
    };
    let pool = euterpe_server::db::connect(&config.database_url)
        .await
        .unwrap();
    euterpe_server::db::migrate(&pool).await.unwrap();

    let (job_tx, _job_rx) = tokio::sync::mpsc::channel(1);
    let (convert_job_tx, _convert_job_rx) = tokio::sync::mpsc::channel(1);
    let (events, _) = tokio::sync::broadcast::channel(1);
    let (scan_events, _) = tokio::sync::broadcast::channel(1);
    let (convert_events, _) = tokio::sync::broadcast::channel(1);
    let state = euterpe_server::AppState::new(
        config,
        pool,
        euterpe_server::AppChannels {
            job_tx,
            convert_job_tx,
            events,
            scan_events,
            convert_events,
        },
        None,
    )
    .await
    .unwrap();
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/storage")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "library": {
                            "kind": "smb",
                            "host": "nas.local",
                            "share": "music",
                            "password": "secret"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

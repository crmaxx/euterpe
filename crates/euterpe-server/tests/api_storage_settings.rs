use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use tempfile::tempdir;
use tower::ServiceExt;

#[tokio::test]
async fn storage_settings_encrypts_smb_password_and_redacts_response() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state.clone());

    let response = app
        .clone()
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
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body_text.contains("secret"));
    assert!(!body_text.contains("password_encrypted"));
    assert!(!body_text.contains("enc:"));
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["kind"], "smb");
    assert_eq!(json["settings"]["library"]["host"], "192.168.0.124");
    assert!(json["settings"]["library"].get("password").is_none());
    assert!(
        json["settings"]["library"]
            .get("password_encrypted")
            .is_none()
    );

    let raw = euterpe_data::fixtures::settings::get(&state.data, "storage.settings")
        .await
        .unwrap()
        .unwrap();
    assert!(!raw.contains("secret"));
    assert!(raw.contains("password_encrypted"));

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/settings/storage")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body = get_response.into_body().collect().await.unwrap().to_bytes();
    let get_body_text = String::from_utf8(get_body.to_vec()).unwrap();
    assert!(!get_body_text.contains("secret"));
    assert!(!get_body_text.contains("password_encrypted"));
    assert!(!get_body_text.contains("enc:"));
}

#[tokio::test]
async fn authenticated_storage_settings_returns_admin_storage_view() {
    let mut state = app::test_support::test_state_without_worker().await;
    let mut config = (*state.config).clone();
    config.admin_password = Some("admin-secret".to_string());
    state.config = std::sync::Arc::new(config);
    let app = app::app(state);

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/storage")
                .header("authorization", "Bearer admin-secret")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "library": {
                            "kind": "smb",
                            "host": "nas.local",
                            "share": "music",
                            "path": "library",
                            "username": "admin-user",
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
    assert_eq!(patch_response.status(), StatusCode::OK);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/settings/storage")
                .header("authorization", "Bearer admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::OK);
    let body = get_response.into_body().collect().await.unwrap().to_bytes();
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body_text.contains("secret"));
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["kind"], "smb");
    assert_eq!(json["settings"]["library"]["host"], "nas.local");
    assert_eq!(json["settings"]["library"]["share"], "music");
    assert_eq!(json["settings"]["library"]["path"], "library");
    assert_eq!(json["settings"]["library"]["username"], "admin-user");
    assert_eq!(json["settings"]["library"]["workgroup"], "WORKGROUP");
    assert_eq!(json["settings"]["library"]["password_configured"], true);
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
    let data = euterpe_data::connect_database(&config.database_url)
        .await
        .unwrap();
    euterpe_data::migrations::migrate(&data).await.unwrap();

    let (job_tx, _job_rx) = tokio::sync::mpsc::channel(1);
    let (convert_job_tx, _convert_job_rx) = tokio::sync::mpsc::channel(1);
    let (events, _) = tokio::sync::broadcast::channel(1);
    let (scan_events, _) = tokio::sync::broadcast::channel(1);
    let (convert_events, _) = tokio::sync::broadcast::channel(1);
    let state = euterpe_server::AppState::new(
        config,
        data,
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

#[tokio::test]
async fn storage_settings_patch_kind_change_returns_migration_hints() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state);

    let local_path = std::env::temp_dir()
        .join(format!("euterpe-storage-kind-{}", std::process::id()))
        .display()
        .to_string();

    let to_smb = app
        .clone()
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
                            "port": 445,
                            "share": "music",
                            "path": "library"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(to_smb.status(), StatusCode::OK);
    let body = to_smb.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["kind"], "smb");
    assert_eq!(json["recommend_full_scan"], true);
    assert!(
        json["storage_migration_hint"]
            .as_str()
            .is_some_and(|h| h.contains("SMB"))
    );

    let to_local = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/storage")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "library": {
                            "kind": "local",
                            "path": local_path
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(to_local.status(), StatusCode::OK);
    let body = to_local.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["kind"], "local");
    assert_eq!(json["recommend_full_scan"], true);
    assert!(
        json["storage_migration_hint"]
            .as_str()
            .is_some_and(|h| h.contains("local"))
    );
}

#[tokio::test]
async fn storage_settings_patch_same_kind_omits_migration_hints() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state.clone());

    let response = app
        .clone()
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
                            "share": "music"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

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
                            "host": "nas2.local",
                            "share": "music2"
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
    assert!(json.get("recommend_full_scan").is_none());
    assert!(json.get("storage_migration_hint").is_none());
}

#[tokio::test]
async fn storage_settings_preserves_smb_password_and_username_on_patch() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state);

    let first = app
        .clone()
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
                            "share": "dietpi",
                            "username": "dietpi",
                            "password": "secret"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let body = first.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["username"], "dietpi");
    assert_eq!(json["settings"]["library"]["password_configured"], true);

    let second = app
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
                            "share": "dietpi",
                            "username": "dietpi"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let body = second.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["password_configured"], true);
}

#[tokio::test]
async fn storage_settings_clears_smb_username_workgroup_and_password_on_explicit_null() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state);

    let first = app
        .clone()
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
                            "username": "music-user",
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
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
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
                            "username": null,
                            "password": null,
                            "workgroup": null
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(second.status(), StatusCode::OK);
    let body = second.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["settings"]["library"].get("username").is_none());
    assert!(json["settings"]["library"].get("workgroup").is_none());
    assert_eq!(json["settings"]["library"]["password_configured"], false);
}

#[tokio::test]
async fn storage_settings_omits_smb_credentials_to_preserve_existing_identity() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state);

    let first = app
        .clone()
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
                            "path": "saved",
                            "username": "music-user",
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
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
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
                            "path": "draft"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(second.status(), StatusCode::OK);
    let body = second.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["username"], "music-user");
    assert_eq!(json["settings"]["library"]["workgroup"], "WORKGROUP");
    assert_eq!(json["settings"]["library"]["password_configured"], true);
}

#[tokio::test]
async fn storage_browse_uses_draft_location_from_request_body() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state);
    let saved_root = tempdir().unwrap();
    let draft_root = tempdir().unwrap();
    tokio::fs::create_dir(draft_root.path().join("draft-only"))
        .await
        .unwrap();

    let save = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/storage")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "library": {
                            "kind": "local",
                            "path": saved_root.path()
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save.status(), StatusCode::OK);

    let browse = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/settings/storage/browse")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "location": {
                            "kind": "local",
                            "path": draft_root.path()
                        },
                        "path": ""
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(browse.status(), StatusCode::OK);
    let body = browse.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let names: Vec<_> = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["draft-only"]);
}

#[tokio::test]
async fn storage_settings_presets_round_trip_and_activate() {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state);

    let local_path = std::env::temp_dir()
        .join(format!("euterpe-storage-preset-{}", std::process::id()))
        .display()
        .to_string();

    let save_local = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/storage")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "library": { "kind": "local", "path": local_path }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save_local.status(), StatusCode::OK);

    let save_smb = app
        .clone()
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
                            "share": "music"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save_smb.status(), StatusCode::OK);
    let body = save_smb.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let presets = json["settings"]["presets"].as_array().unwrap();
    assert!(presets.len() >= 2);

    let local_preset_id = presets
        .iter()
        .find(|p| p["kind"] == "local")
        .and_then(|p| p["id"].as_str())
        .unwrap();

    let activate = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings/storage")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "activate_preset_id": local_preset_id }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(activate.status(), StatusCode::OK);
    let body = activate.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["settings"]["library"]["kind"], "local");
}

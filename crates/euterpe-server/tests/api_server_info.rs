use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use euterpe_server::services::app_settings::{self, StorageLocation, StorageSettings};
use http_body_util::BodyExt;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;

#[path = "support/schema.rs"]
mod schema;

#[tokio::test]
async fn server_info_returns_config_snapshot() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/server/info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let spec = schema::load_spec();
    schema::validate_schema(
        &schema::schema_from_spec(&spec, "ServerInfoResponse"),
        &json,
    );
    assert_eq!(json["library_storage"]["kind"], "local");
    assert_eq!(json["library_storage"]["watch_status"]["state"], "disabled");
    assert!(json.get("library_path").is_none());
    assert!(json["credentials_configured"].is_boolean());
    assert!(json["admin_auth_required"].is_boolean());
}

#[tokio::test]
async fn server_info_returns_smb_storage_summary_without_credentials() {
    let state = app::test_support::test_state_without_worker().await;
    app_settings::save_storage(
        &state.data,
        &StorageSettings {
            library: Some(StorageLocation::Smb {
                host: "nas.secret.lan".to_string(),
                port: 445,
                share: "private_music".to_string(),
                path: "Users/admin/Music".to_string(),
                username: Some("admin-user".to_string()),
                password_encrypted: Some("enc:configured".to_string()),
                workgroup: Some("SECRETWG".to_string()),
            }),
            presets: Vec::new(),
        },
    )
    .await
    .unwrap();
    {
        let mut runtime = state.runtime.write().await;
        runtime.storage = app_settings::load_storage(&state.data, &state.config).await;
    }
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/server/info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body_text.contains("admin-user"));
    assert!(!body_text.contains("SECRETWG"));
    assert!(!body_text.contains("enc:configured"));
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["library_storage"]["kind"], "smb");
    assert_eq!(json["library_storage"]["host"], "nas.secret.lan");
    assert_eq!(json["library_storage"]["port"], 445);
    assert_eq!(json["library_storage"]["share"], "private_music");
    assert_eq!(json["library_storage"]["path"], "Users/admin/Music");
    assert!(json["library_storage"].get("username").is_none());
    assert!(json["library_storage"].get("workgroup").is_none());
    assert_eq!(json["library_storage"]["password_configured"], false);
}

#[tokio::test]
async fn server_info_returns_local_storage_summary() {
    let state = app::test_support::test_state_without_worker().await;
    let absolute_path = state.config.library_path.display().to_string();
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/server/info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["library_storage"]["kind"], "local");
    assert_eq!(json["library_storage"]["path"], absolute_path);
    assert_eq!(json["library_storage"]["watch_status"]["state"], "disabled");
}

#[tokio::test]
async fn request_logging_redacts_events_query_access_token() {
    let output = capture_request_logs("/api/v1/events?access_token=secret-token").await;

    assert!(!output.contains("secret-token"));
    assert!(output.contains("/api/v1/events"));
}

#[tokio::test]
async fn request_logging_redacts_stream_query_access_token() {
    let output =
        capture_request_logs("/api/v1/library/tracks/123/stream?access_token=secret-token").await;

    assert!(!output.contains("secret-token"));
    assert!(output.contains("/api/v1/library/tracks/123/stream"));
}

async fn capture_request_logs(uri: &str) -> String {
    let state = app::test_support::test_state_without_worker().await;
    let app = app::app(state);
    let sink = SharedLogSink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(sink.clone())
        .finish();

    let guard = tracing::subscriber::set_default(subscriber);
    async {
        let _ = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
    }
    .await;
    drop(guard);

    sink.contents()
}

#[derive(Clone, Default)]
struct SharedLogSink {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl SharedLogSink {
    fn contents(&self) -> String {
        let bytes = self.bytes.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }
}

impl<'a> MakeWriter<'a> for SharedLogSink {
    type Writer = SharedLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SharedLogWriter {
            bytes: self.bytes.clone(),
        }
    }
}

struct SharedLogWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl Write for SharedLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn sync_latest_returns_null_when_no_runs() {
    let state = app::test_support::test_state().await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/sync/latest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let spec = schema::load_spec();
    schema::validate_schema(
        &schema::schema_from_spec(&spec, "QobuzSyncLatestResponse"),
        &json,
    );
    assert!(json["run"].is_null());
}

#[tokio::test]
async fn sync_latest_returns_most_recent_run() {
    let state = app::test_support::test_state().await;
    let completed = euterpe_data::repositories::qobuz::start_sync_run(&state.data)
        .await
        .unwrap();
    euterpe_data::repositories::qobuz::finish_sync_success(&state.data, completed, 10, 1, 0)
        .await
        .unwrap();
    let _running = euterpe_data::repositories::qobuz::start_sync_run(&state.data)
        .await
        .unwrap();

    let app = app::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/sync/latest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["run"]["status"], "running");
    assert!(
        json["run"]["started_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
}

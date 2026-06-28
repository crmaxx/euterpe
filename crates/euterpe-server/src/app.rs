use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use euterpe_data::{
    connect_database, migrations as data_migrations,
    repositories::{favorites, qobuz as qobuz_runs},
};
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::library::covers::MAX_ALBUM_COVER_BYTES;
use tracing::Level;

use crate::api::{
    HealthResponse, QobuzFavoriteItem, QobuzFavoritesListResponse, QobuzFavoritesMutateRequest,
    QobuzSyncLatestResponse, QobuzSyncResponse, QobuzSyncRunSummary, QobuzTestLoginRequest,
    QobuzTestLoginResponse, ServerInfoResponse, SortKeyKind, SortKeyValue, SortOrder,
    StorageLocationView, StorageSettingsView,
};
use crate::config::AppConfig;
use crate::credentials;
use crate::error::ApiError;
use crate::middleware;
use crate::openapi;
use crate::routes::{
    downloads, events, integrations, library, qobuz as qobuz_routes, settings, settings_ext,
    torrent,
};
use crate::services::convert::{ConvertWorkerDeps, spawn_convert_worker};
use crate::services::download::{WorkerDeps, spawn_worker};
use crate::services::qobuz_sync;
use crate::state::{AppChannels, AppState};

pub fn app(state: AppState) -> Router {
    let http_debug = state.config.debug;
    let hawk = state.hawk.clone();
    let protected = Router::new()
        .route("/api/v1/qobuz/oauth/start", get(qobuz_routes::oauth_start))
        .route("/api/v1/qobuz/accounts", get(qobuz_routes::list_accounts))
        .route(
            "/api/v1/qobuz/connection",
            get(qobuz_routes::connection_status),
        )
        .route("/api/v1/qobuz/logout", post(qobuz_routes::logout))
        .route("/api/v1/qobuz/sync/latest", get(qobuz_sync_latest))
        .route("/api/v1/qobuz/test-login", post(qobuz_test_login))
        .route("/api/v1/qobuz/sync", post(qobuz_sync_handler))
        .route(
            "/api/v1/qobuz/favorites",
            get(list_favorites)
                .post(add_favorites)
                .delete(remove_favorites),
        )
        .route(
            "/api/v1/downloads",
            post(downloads::create_download).get(downloads::list_downloads),
        )
        .route(
            "/api/v1/downloads/by-url",
            post(downloads::create_download_by_url),
        )
        .route(
            "/api/v1/downloads/purge",
            post(downloads::purge_finished_downloads),
        )
        .route(
            "/api/v1/downloads/{id}",
            get(downloads::get_download).delete(downloads::delete_download),
        )
        .route(
            "/api/v1/downloads/{id}/priority",
            axum::routing::patch(downloads::patch_download_priority),
        )
        .route(
            "/api/v1/downloads/{id}/retry",
            axum::routing::post(downloads::retry_download),
        )
        .route(
            "/api/v1/downloads/{id}/pause",
            axum::routing::post(downloads::pause_download),
        )
        .route(
            "/api/v1/downloads/{id}/resume",
            axum::routing::post(downloads::resume_download),
        )
        .route(
            "/api/v1/downloads/torrent/inspect",
            post(torrent::inspect_torrent_magnet),
        )
        .route(
            "/api/v1/downloads/torrent/inspect/file",
            post(torrent::inspect_torrent_file),
        )
        .route(
            "/api/v1/downloads/torrent/confirm",
            post(torrent::confirm_torrent),
        )
        .route(
            "/api/v1/settings/torrent",
            get(settings::get_torrent_settings).patch(settings::patch_torrent_settings),
        )
        .route(
            "/api/v1/settings/ui",
            get(settings_ext::get_ui_settings).patch(settings_ext::patch_ui_settings),
        )
        .route(
            "/api/v1/settings/converter",
            get(settings_ext::get_converter_settings).patch(settings_ext::patch_converter_settings),
        )
        .route(
            "/api/v1/settings/library-scan",
            get(settings_ext::get_library_scan_settings)
                .patch(settings_ext::patch_library_scan_settings),
        )
        .route(
            "/api/v1/settings/downloads",
            get(settings_ext::get_downloads_settings).patch(settings_ext::patch_downloads_settings),
        )
        .route(
            "/api/v1/settings/qobuz-scheduled-sync",
            get(settings_ext::get_qobuz_scheduled_sync_settings)
                .patch(settings_ext::patch_qobuz_scheduled_sync_settings),
        )
        .route(
            "/api/v1/settings/qobuz-scheduled-sync/run",
            post(settings_ext::run_qobuz_scheduled_sync_now),
        )
        .route(
            "/api/v1/settings/storage",
            get(settings_ext::get_storage_settings).patch(settings_ext::patch_storage_settings),
        )
        .route(
            "/api/v1/settings/storage/test",
            post(settings_ext::test_storage_settings),
        )
        .route(
            "/api/v1/settings/storage/browse",
            get(settings_ext::browse_storage).post(settings_ext::browse_storage_draft),
        )
        .route(
            "/api/v1/settings/storage/smb-shares",
            post(settings_ext::list_smb_shares),
        )
        .route("/api/v1/library/scan", post(library::start_library_scan))
        .route(
            "/api/v1/library/scan/latest",
            get(library::library_scan_latest),
        )
        .route(
            "/api/v1/library/scan/{id}",
            get(library::get_library_scan).delete(library::cancel_library_scan),
        )
        .route("/api/v1/library/albums", get(library::list_library_albums))
        .route(
            "/api/v1/library/albums/{id}",
            get(library::get_library_album).patch(library::patch_library_album_tags),
        )
        .route(
            "/api/v1/library/albums/{id}/convert",
            post(library::post_library_album_convert),
        )
        .route(
            "/api/v1/library/albums/{id}/convert/latest",
            get(library::get_library_album_convert_latest),
        )
        .route(
            "/api/v1/library/albums/{id}/cue",
            get(library::get_library_album_cue),
        )
        .route(
            "/api/v1/library/albums/{id}/cue/validate",
            post(library::validate_library_album_cue),
        )
        .route(
            "/api/v1/library/albums/{id}/cue/split",
            post(library::split_library_album_cue),
        )
        .route(
            "/api/v1/library/albums/{id}/cue/latest",
            get(library::get_library_album_cue_latest),
        )
        .route(
            "/api/v1/library/convert/jobs/{id}",
            get(library::get_convert_job),
        )
        .route(
            "/api/v1/library/albums/{id}/cover",
            get(library::get_library_album_cover).put(library::put_library_album_cover),
        )
        .layer(RequestBodyLimitLayer::new(MAX_ALBUM_COVER_BYTES))
        .route(
            "/api/v1/library/tracks/{id}/stream",
            get(library::get_library_track_stream),
        )
        .route(
            "/api/v1/library/tracks/{id}",
            get(library::get_library_track).patch(library::patch_library_track_tags),
        )
        .route(
            "/api/v1/library/albums/{id}/metadata/lookup",
            post(integrations::album_metadata_lookup),
        )
        .route(
            "/api/v1/library/albums/{id}/metadata/apply",
            post(integrations::album_metadata_apply),
        )
        .route(
            "/api/v1/integrations",
            get(integrations::list_integrations).post(integrations::create_integration),
        )
        .route(
            "/api/v1/integrations/catalog",
            get(integrations::integrations_catalog),
        )
        .route(
            "/api/v1/integrations/{id}",
            axum::routing::patch(integrations::patch_integration)
                .delete(integrations::delete_integration),
        )
        .route("/api/v1/events", get(events::subscribe_events))
        .layer(axum::middleware::from_fn_with_state(
            state.config.clone(),
            middleware::admin_auth,
        ));

    let mut router = Router::new()
        .route("/health", get(health))
        .route("/api/openapi.json", get(openapi_json))
        .route("/api/v1/server/info", get(server_info))
        .route(
            "/api/v1/qobuz/oauth/callback",
            get(qobuz_routes::oauth_callback),
        )
        .merge(protected);

    router = crate::static_files::apply_fallback(router, &state.config);

    let mut router = router.with_state(state);

    if let Some(hawk) = hawk {
        router = euterpe_hawk::axum::apply_layers(router, hawk);
    }

    router
        .layer(axum::middleware::from_fn(move |req, next| {
            let debug = http_debug;
            async move { middleware::log_http_error_response(debug, req, next).await }
        }))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &Request<Body>| {
                    tracing::info_span!(
                        "http",
                        method = %req.method(),
                        uri = %middleware::request_log_uri(req.uri()),
                    )
                })
                .on_request(|req: &Request<Body>, _span: &tracing::Span| {
                    tracing::event!(
                        Level::DEBUG,
                        method = %req.method(),
                        uri = %middleware::request_log_uri(req.uri()),
                        "request started"
                    );
                })
                .on_response(
                    |res: &Response<Body>, latency: Duration, _span: &tracing::Span| {
                        let status = res.status().as_u16();
                        if status < 400 {
                            middleware::log_http_success(status, latency.as_millis() as u64);
                        }
                    },
                )
                .on_failure(
                    |_failure: tower_http::classify::ServerErrorsFailureClass,
                     _latency: Duration,
                     _span: &tracing::Span| {
                        // Error body + code are logged in middleware when EUTERPE_DEBUG is set.
                    },
                ),
        )
}

pub async fn serve(
    config: AppConfig,
    hawk: Option<std::sync::Arc<euterpe_hawk::Hawk>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    config.ensure_torrent_incoming_dir()?;

    let data = connect_database(&config.database_url).await?;
    data_migrations::migrate(&data).await?;

    let (job_tx, job_rx) = mpsc::channel(32);
    let (convert_job_tx, convert_job_rx) = mpsc::channel(32);
    let (events, _) = broadcast::channel(64);
    let (scan_events, _) = broadcast::channel(64);
    let (convert_events, _) = broadcast::channel(64);

    let bind = config.bind;
    let config = Arc::new(config);
    let state = AppState::new(
        (*config).clone(),
        data.clone(),
        AppChannels {
            job_tx: job_tx.clone(),
            convert_job_tx: convert_job_tx.clone(),
            events: events.clone(),
            scan_events,
            convert_events: convert_events.clone(),
        },
        hawk.clone(),
    )
    .await?;
    state.storage_watch.restart().await;
    state.qobuz_scheduled_sync.restart().await?;

    let worker_deps = WorkerDeps {
        data: data.clone(),
        qobuz: Arc::clone(&state.qobuz),
        config: Arc::clone(&state.config),
        runtime: state.runtime.clone(),
        events,
        http: Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?,
        torrent: state.torrent.clone(),
        torrent_semaphore: state
            .torrent
            .as_ref()
            .map(|_| Arc::new(tokio::sync::Semaphore::new(state.config.torrent_max_active))),
        scan_events: state.scan_events.clone(),
        job_tx: job_tx.clone(),
        convert_job_tx: convert_job_tx.clone(),
    };
    spawn_worker(job_rx, worker_deps);

    let convert_deps = ConvertWorkerDeps {
        data: data.clone(),
        config: Arc::clone(&state.config),
        runtime: state.runtime.clone(),
        events: convert_events,
        scan_events: state.scan_events.clone(),
        job_tx: convert_job_tx.clone(),
    };
    spawn_convert_worker(convert_job_rx, convert_deps);

    let _ = job_tx.send(0).await;
    let _ = convert_job_tx.send(0).await;

    let router = app(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    if config.debug {
        tracing::info!(
            bind = %bind,
            "euterpe debug logging enabled (EUTERPE_DEBUG): HTTP, Qobuz API, library scan, download workers; set RUST_LOG to override"
        );
    }
    tracing::info!("listening on {}", bind);
    axum::serve(listener, router).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn server_info(State(state): State<AppState>) -> Result<Json<ServerInfoResponse>, ApiError> {
    let credentials_configured = credentials::load_active(&state.config, &state.data)
        .await?
        .is_some();
    let runtime = state.runtime.read().await;
    let ui = runtime.ui.clone();
    let storage = runtime.storage.clone();
    drop(runtime);
    let watch_status = state.storage_watch.status().await;
    let library_storage = StorageSettingsView::from_with_watch_status(&storage, watch_status)
        .library
        .map(public_server_storage_view);
    Ok(Json(ServerInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        library_storage,
        torrent_incoming_dir: state
            .config
            .torrent_incoming_dir
            .as_ref()
            .map(|p| p.display().to_string()),
        credentials_configured,
        admin_auth_required: state.config.admin_password.is_some(),
        ui,
    }))
}

fn public_server_storage_view(location: StorageLocationView) -> StorageLocationView {
    match location {
        StorageLocationView::Smb {
            host,
            port,
            share,
            path,
            watch_status,
            ..
        } => StorageLocationView::Smb {
            host,
            port,
            share,
            path,
            watch_status,
            username: None,
            workgroup: None,
            password_configured: false,
        },
        other => other,
    }
}

async fn qobuz_sync_latest(
    State(state): State<AppState>,
) -> Result<Json<QobuzSyncLatestResponse>, ApiError> {
    let run = qobuz_runs::sync_latest(&state.data)
        .await?
        .map(qobuz_sync_run_from_data);
    Ok(Json(QobuzSyncLatestResponse { run }))
}

async fn openapi_json() -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        openapi::spec_json().map_err(|e| ApiError::Config(e.to_string()))?,
    ))
}

async fn qobuz_test_login(
    Json(body): Json<QobuzTestLoginRequest>,
) -> Result<Json<QobuzTestLoginResponse>, ApiError> {
    let client = credentials::connect_ephemeral(body.user_id, &body.auth_token).await?;
    client.verify_session().await?;

    Ok(Json(QobuzTestLoginResponse {
        membership: credentials::membership_label(&client),
        user_auth_token_refreshed: false,
    }))
}

async fn qobuz_sync_handler(
    State(state): State<AppState>,
) -> Result<Json<QobuzSyncResponse>, ApiError> {
    tracing::debug!("POST /api/v1/qobuz/sync");
    state.require_credentials().await?;
    let resp = qobuz_sync::run(&state.data, Arc::clone(&state.qobuz)).await?;
    tracing::debug!(
        run_id = resp.run_id,
        albums_total = resp.albums_total,
        added = resp.added,
        removed = resp.removed,
        "sync complete"
    );
    Ok(Json(resp))
}

#[derive(Debug, Deserialize)]
struct FavoritesQuery {
    #[serde(rename = "type")]
    entity_type: String,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default = "default_favorites_sort")]
    sort: String,
    #[serde(default)]
    order: Option<String>,
    cursor: Option<String>,
    q: Option<String>,
    in_library: Option<bool>,
}

fn default_limit() -> u32 {
    50
}

fn default_favorites_sort() -> String {
    "title".to_string()
}

async fn list_favorites(
    State(state): State<AppState>,
    Query(q): Query<FavoritesQuery>,
) -> Result<Json<QobuzFavoritesListResponse>, ApiError> {
    if q.entity_type != "album" {
        return Err(ApiError::bad_request("only type=album is supported"));
    }
    use crate::api::keyset::parse_limit;

    let limit = parse_limit(q.limit, 50, 500)?;
    let sort = favorites::FavoritesSort::parse(&q.sort)?;
    let order = match q.order.as_deref() {
        None => favorites::SortOrder::Asc,
        Some("asc") => favorites::SortOrder::Asc,
        Some("desc") => favorites::SortOrder::Desc,
        Some(_) => return Err(ApiError::bad_request("order must be asc or desc")),
    };
    let fingerprint = qobuz_favorites_fingerprint(q.q.as_ref(), q.in_library);
    let after = decode_qobuz_favorites_cursor(&q, sort, order, &fingerprint)?;
    let page = favorites::list_albums_keyset(
        &state.data,
        favorites::FavoritesListParams {
            sort,
            order,
            limit: limit as usize,
            q: q.q,
            in_library: q.in_library,
            after,
        },
    )
    .await?;
    let next_cursor = page
        .next_after
        .as_ref()
        .map(|cursor| encode_qobuz_favorites_cursor(sort, order, &fingerprint, cursor));
    Ok(Json(QobuzFavoritesListResponse {
        items: page
            .items
            .into_iter()
            .map(qobuz_favorite_item_from_data)
            .collect(),
        next_cursor,
        has_more: page.has_more,
    }))
}

async fn add_favorites(
    State(state): State<AppState>,
    Json(body): Json<QobuzFavoritesMutateRequest>,
) -> Result<StatusCode, ApiError> {
    state.require_credentials().await?;
    if body.album_ids.is_empty() {
        return Err(ApiError::bad_request("album_ids must not be empty"));
    }
    {
        let guard = state.qobuz.lock().await;
        guard.favorite_add_albums(&body.album_ids).await?;
    }
    for &id in &body.album_ids {
        favorites::upsert_album(&state.data, id, "", "", None, None).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_favorites(
    State(state): State<AppState>,
    Json(body): Json<QobuzFavoritesMutateRequest>,
) -> Result<StatusCode, ApiError> {
    state.require_credentials().await?;
    if body.album_ids.is_empty() {
        return Err(ApiError::bad_request("album_ids must not be empty"));
    }
    {
        let guard = state.qobuz.lock().await;
        guard.favorite_remove_albums(&body.album_ids).await?;
    }
    favorites::mark_albums_removed(&state.data, &body.album_ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn qobuz_favorites_fingerprint(q: Option<&String>, in_library: Option<bool>) -> String {
    crate::api::keyset::fingerprint_json(&serde_json::json!({
        "q": q,
        "in_library": in_library,
    }))
}

fn decode_qobuz_favorites_cursor(
    query: &FavoritesQuery,
    sort: favorites::FavoritesSort,
    order: favorites::SortOrder,
    fingerprint: &str,
) -> Result<Option<favorites::FavoriteListCursor>, ApiError> {
    let Some(cursor_str) = query.cursor.as_deref() else {
        return Ok(None);
    };
    let api_order = api_sort_order(order);
    let payload = crate::api::keyset::decode_cursor(cursor_str)?;
    let (primary, tie_qobuz_id) = crate::api::keyset::ensure_cursor_matches(
        &payload,
        data_favorite_sort_str(sort),
        api_order,
        fingerprint,
        favorite_sort_key_kind(sort),
    )?;
    Ok(Some(favorites::FavoriteListCursor {
        primary: data_favorite_sort_value(primary),
        tie_qobuz_id,
    }))
}

fn encode_qobuz_favorites_cursor(
    sort: favorites::FavoritesSort,
    order: favorites::SortOrder,
    fingerprint: &str,
    cursor: &favorites::FavoriteListCursor,
) -> String {
    crate::api::keyset::encode_cursor(
        data_favorite_sort_str(sort),
        api_sort_order(order),
        fingerprint,
        &api_favorite_sort_value(&cursor.primary),
        cursor.tie_qobuz_id,
    )
}

fn data_favorite_sort_str(sort: favorites::FavoritesSort) -> &'static str {
    match sort {
        favorites::FavoritesSort::Title => "title",
        favorites::FavoritesSort::Artist => "artist",
        favorites::FavoritesSort::InLibrary => "in_library",
    }
}

fn favorite_sort_key_kind(sort: favorites::FavoritesSort) -> SortKeyKind {
    match sort {
        favorites::FavoritesSort::InLibrary => SortKeyKind::Bool,
        favorites::FavoritesSort::Title | favorites::FavoritesSort::Artist => SortKeyKind::Text,
    }
}

fn api_sort_order(order: favorites::SortOrder) -> SortOrder {
    match order {
        favorites::SortOrder::Asc => SortOrder::Asc,
        favorites::SortOrder::Desc => SortOrder::Desc,
    }
}

fn data_favorite_sort_value(value: SortKeyValue) -> favorites::FavoriteSortValue {
    match value {
        SortKeyValue::Text(text) => favorites::FavoriteSortValue::Text(text),
        SortKeyValue::Bool(value) => favorites::FavoriteSortValue::Bool(value),
        SortKeyValue::Int(value) => favorites::FavoriteSortValue::Text(value.to_string()),
    }
}

fn api_favorite_sort_value(value: &favorites::FavoriteSortValue) -> SortKeyValue {
    match value {
        favorites::FavoriteSortValue::Text(text) => SortKeyValue::Text(text.clone()),
        favorites::FavoriteSortValue::Bool(value) => SortKeyValue::Bool(*value),
    }
}

fn qobuz_favorite_item_from_data(row: favorites::QobuzFavoriteAlbum) -> QobuzFavoriteItem {
    QobuzFavoriteItem {
        album_api_id: row.album_api_id,
        qobuz_id: row.qobuz_id,
        title: row.title,
        artist_name: row.artist_name,
        in_library: row.in_library,
        local_album_id: row.local_album_id,
        local_cover_path: row.local_cover_path,
        cover_url: row.cover_url,
    }
}

fn qobuz_sync_run_from_data(row: qobuz_runs::QobuzSyncRunSummary) -> QobuzSyncRunSummary {
    QobuzSyncRunSummary {
        id: row.id,
        status: row.status,
        trigger: row.trigger,
        started_at: row.started_at,
        finished_at: row.finished_at,
        albums_total: row.albums_total,
        albums_added: row.albums_added,
        albums_removed: row.albums_removed,
        error_message: row.error_message,
        skip_reason: row.skip_reason,
    }
}

pub mod test_support {
    use super::*;
    use crate::services::download::{WorkerDeps, spawn_worker};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_CONFIG_ID: AtomicU64 = AtomicU64::new(0);

    fn test_config() -> AppConfig {
        let id = TEST_CONFIG_ID.fetch_add(1, Ordering::Relaxed);
        let library_path =
            std::env::temp_dir().join(format!("euterpe-server-test-{}-{id}", std::process::id()));
        AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: Some(crate::crypto::MasterKey::parse(&hex::encode([1u8; 32])).unwrap()),
            public_base_url: "http://127.0.0.1:0".into(),
            oauth_state_ttl: std::time::Duration::from_secs(600),
            qobuz_api_base: None,
            qobuz_play_base: None,
            library_path,
            torrent_incoming_dir: None,
            torrent_max_active: 2,
            torrent_enable_upnp: false,
            download_concurrency: 2,
            library_scan: crate::config::LibraryScanConfig::default(),
            debug: false,
            static_dir: std::path::PathBuf::new(),
        }
    }

    async fn test_state_inner(with_worker: bool) -> AppState {
        let config = test_config();
        let data = connect_database(&config.database_url).await.unwrap();
        data_migrations::migrate(&data).await.unwrap();
        crate::services::app_settings::save_storage(
            &data,
            &crate::services::app_settings::StorageSettings::local(
                config.library_path.display().to_string(),
            ),
        )
        .await
        .unwrap();

        let (job_tx, job_rx) = mpsc::channel(32);
        let (convert_job_tx, _convert_job_rx) = mpsc::channel(32);
        let (events, _) = broadcast::channel(16);
        let (scan_events, _) = broadcast::channel(16);
        let (convert_events, _) = broadcast::channel(16);

        let state = AppState::new(
            config.clone(),
            data.clone(),
            AppChannels {
                job_tx,
                convert_job_tx: convert_job_tx.clone(),
                events: events.clone(),
                scan_events,
                convert_events,
            },
            None,
        )
        .await
        .unwrap();

        if with_worker {
            let job_tx_wake = state.job_tx.clone();
            spawn_worker(
                job_rx,
                WorkerDeps {
                    data: data.clone(),
                    qobuz: Arc::clone(&state.qobuz),
                    config: Arc::new(config),
                    runtime: state.runtime.clone(),
                    events,
                    http: Client::new(),
                    torrent: None,
                    torrent_semaphore: None,
                    scan_events: state.scan_events.clone(),
                    job_tx: job_tx_wake.clone(),
                    convert_job_tx,
                },
            );
            let _ = job_tx_wake.send(0).await;
        }

        state
    }

    pub async fn test_state() -> AppState {
        test_state_inner(true).await
    }

    /// App state for API tests that seed `download_jobs` directly (no background scheduler).
    pub async fn test_state_without_worker() -> AppState {
        test_state_inner(false).await
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_config_uses_unique_library_paths() {
            let first = test_config();
            let second = test_config();

            assert_ne!(first.library_path, second.library_path);
        }
    }
}

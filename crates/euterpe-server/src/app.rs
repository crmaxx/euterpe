use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::library::covers::MAX_ALBUM_COVER_BYTES;
use tracing::Level;

use crate::api::{
    HealthResponse, QobuzFavoritesListResponse, QobuzFavoritesMutateRequest,
    QobuzSyncLatestResponse, QobuzSyncResponse, QobuzTestLoginRequest, QobuzTestLoginResponse,
    ServerInfoResponse,
};
use crate::config::AppConfig;
use crate::credentials;
use crate::db::{self, favorites, sync_runs};
use crate::error::ApiError;
use crate::middleware;
use crate::openapi;
use crate::routes::{
    downloads, events, integrations, library, qobuz as qobuz_routes, settings, torrent,
};
use crate::services::download::{WorkerDeps, spawn_worker};
use crate::services::qobuz_sync;
use crate::state::AppState;

pub fn app(state: AppState) -> Router {
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

    router.layer(
        TraceLayer::new_for_http()
            .make_span_with(|req: &Request<Body>| {
                tracing::info_span!(
                    "http",
                    method = %req.method(),
                    uri = %req.uri(),
                )
            })
            .on_request(|req: &Request<Body>, _span: &tracing::Span| {
                tracing::event!(
                    Level::DEBUG,
                    method = %req.method(),
                    uri = %req.uri(),
                    "request started"
                );
            })
            .on_response(
                |res: &Response<Body>, latency: Duration, _span: &tracing::Span| {
                    let status = res.status().as_u16();
                    let latency_ms = latency.as_millis() as u64;
                    if status >= 400 {
                        tracing::event!(
                            Level::WARN,
                            status,
                            latency_ms,
                            "response failed — error JSON is in the response body"
                        );
                    } else {
                        tracing::event!(Level::DEBUG, status, latency_ms, "response");
                    }
                },
            ),
    )
}

pub async fn serve(
    config: AppConfig,
    hawk: Option<std::sync::Arc<euterpe_hawk::Hawk>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    config.ensure_library_root()?;
    config.ensure_torrent_incoming_dir()?;

    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    let (job_tx, job_rx) = mpsc::channel(32);
    let (events, _) = broadcast::channel(64);
    let (scan_events, _) = broadcast::channel(64);

    let bind = config.bind;
    let config = Arc::new(config);
    let state = AppState::new(
        (*config).clone(),
        pool.clone(),
        job_tx.clone(),
        events.clone(),
        scan_events,
        hawk.clone(),
    )
    .await?;

    let worker_deps = WorkerDeps {
        pool,
        qobuz: Arc::clone(&state.qobuz),
        config: Arc::clone(&state.config),
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
    };
    spawn_worker(job_rx, worker_deps);
    let _ = job_tx.send(0).await;

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
    let credentials_configured = credentials::load_active(&state.config, &state.db)
        .await?
        .is_some();
    Ok(Json(ServerInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        library_path: state.config.library_path.display().to_string(),
        torrent_incoming_dir: state
            .config
            .torrent_incoming_dir
            .as_ref()
            .map(|p| p.display().to_string()),
        credentials_configured,
        admin_auth_required: state.config.admin_password.is_some(),
    }))
}

async fn qobuz_sync_latest(
    State(state): State<AppState>,
) -> Result<Json<QobuzSyncLatestResponse>, ApiError> {
    let run = sync_runs::latest(&state.db).await?;
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
    let resp = qobuz_sync::run(&state.db, Arc::clone(&state.qobuz)).await?;
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
    use crate::api::SortOrder;
    use crate::api::keyset::parse_limit;
    use crate::db::favorites::{FavoritesListParams, FavoritesSort};

    let limit = parse_limit(q.limit, 50, 500)?;
    let sort = FavoritesSort::parse(&q.sort)?;
    let order = match q.order.as_deref() {
        None => SortOrder::Asc,
        Some(s) => SortOrder::parse(s)?,
    };
    let page = favorites::list_albums_keyset(
        &state.db,
        FavoritesListParams {
            sort,
            order,
            limit,
            q: q.q,
            in_library: q.in_library,
            cursor: q.cursor,
        },
    )
    .await?;
    Ok(Json(QobuzFavoritesListResponse {
        items: page.items,
        next_cursor: page.next_cursor,
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
        favorites::upsert_album(&state.db, id, "", "", None, None).await?;
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
    favorites::mark_albums_removed(&state.db, &body.album_ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub mod test_support {
    use super::*;
    use crate::db;
    use crate::services::download::{WorkerDeps, spawn_worker};

    pub async fn test_state() -> AppState {
        let library_path =
            std::env::temp_dir().join(format!("euterpe-server-test-{}", std::process::id()));
        let config = AppConfig {
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
        };
        let pool = db::connect(&config.database_url).await.unwrap();
        db::migrate(&pool).await.unwrap();

        let (job_tx, job_rx) = mpsc::channel(32);
        let (events, _) = broadcast::channel(16);
        let (scan_events, _) = broadcast::channel(16);

        let state = AppState::new(
            config.clone(),
            pool.clone(),
            job_tx,
            events.clone(),
            scan_events,
            None,
        )
        .await
        .unwrap();

        let job_tx_wake = state.job_tx.clone();
        spawn_worker(
            job_rx,
            WorkerDeps {
                pool,
                qobuz: Arc::clone(&state.qobuz),
                config: Arc::new(config),
                events,
                http: Client::new(),
                torrent: None,
                torrent_semaphore: None,
                scan_events: state.scan_events.clone(),
                job_tx: job_tx_wake.clone(),
            },
        );
        let _ = job_tx_wake.send(0).await;

        state
    }
}

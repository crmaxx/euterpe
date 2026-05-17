use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc};
use tower_http::trace::TraceLayer;

use crate::api::{
    HealthResponse, QobuzFavoritesListResponse, QobuzFavoritesMutateRequest, QobuzSyncResponse,
    QobuzTestLoginRequest, QobuzTestLoginResponse,
};
use crate::config::AppConfig;
use crate::credentials::{self, QobuzCredentials};
use crate::db::{self, favorites};
use crate::error::ApiError;
use crate::middleware;
use crate::openapi;
use crate::routes::{downloads, events};
use crate::services::download::{spawn_worker, WorkerDeps};
use crate::services::qobuz_sync;
use crate::state::AppState;

pub fn app(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/v1/qobuz/test-login", post(qobuz_test_login))
        .route("/api/v1/qobuz/sync", post(qobuz_sync_handler))
        .route(
            "/api/v1/qobuz/favorites",
            get(list_favorites)
                .post(add_favorites)
                .delete(remove_favorites),
        )
        .route("/api/v1/downloads", post(downloads::create_download).get(downloads::list_downloads))
        .route(
            "/api/v1/downloads/{id}",
            get(downloads::get_download).delete(downloads::cancel_download),
        )
        .route("/api/v1/events", get(events::subscribe_events))
        .layer(axum::middleware::from_fn_with_state(
            state.config.clone(),
            middleware::admin_auth,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/api/openapi.json", get(openapi_json))
        .merge(protected)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub async fn serve(config: AppConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    config.ensure_library_root()?;

    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    let (job_tx, job_rx) = mpsc::channel(32);
    let (events, _) = broadcast::channel(64);

    let bind = config.bind;
    let config = Arc::new(config);
    let state = AppState::new(
        (*config).clone(),
        pool.clone(),
        job_tx,
        events.clone(),
    )
    .await?;

    let worker_deps = WorkerDeps {
        pool,
        qobuz: Arc::clone(&state.qobuz),
        config: Arc::clone(&state.config),
        events,
        http: Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()?,
    };
    spawn_worker(job_rx, worker_deps);

    let router = app(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
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

async fn openapi_json() -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(openapi::spec_json().map_err(|e| ApiError::Config(e.to_string()))?))
}

async fn qobuz_test_login(
    State(state): State<AppState>,
    Json(body): Json<QobuzTestLoginRequest>,
) -> Result<Json<QobuzTestLoginResponse>, ApiError> {
    let client = credentials::connect_ephemeral(body.user_id, &body.auth_token).await?;
    client.verify_session().await?;

    if body.persist {
        let master = state.master_key()?;
        let creds = QobuzCredentials {
            user_id: body.user_id,
            auth_token: body.auth_token.clone(),
        };
        credentials::persist(&state.db, master, &creds).await?;
    }

    Ok(Json(QobuzTestLoginResponse {
        membership: credentials::membership_label(&client),
        user_auth_token_refreshed: false,
    }))
}

async fn qobuz_sync_handler(
    State(state): State<AppState>,
) -> Result<Json<QobuzSyncResponse>, ApiError> {
    state.require_credentials().await?;
    let resp = qobuz_sync::run(&state.db, Arc::clone(&state.qobuz)).await?;
    Ok(Json(resp))
}

#[derive(Debug, Deserialize)]
struct FavoritesQuery {
    #[serde(rename = "type")]
    entity_type: String,
    #[serde(default)]
    page: u32,
    #[serde(default = "default_limit")]
    limit: u32,
}

fn default_limit() -> u32 {
    50
}

async fn list_favorites(
    State(state): State<AppState>,
    Query(q): Query<FavoritesQuery>,
) -> Result<Json<QobuzFavoritesListResponse>, ApiError> {
    if q.entity_type != "album" {
        return Err(ApiError::bad_request("only type=album is supported"));
    }
    if q.limit == 0 || q.limit > 500 {
        return Err(ApiError::bad_request("limit must be 1..=500"));
    }
    let (items, total) = favorites::list_albums(&state.db, q.page, q.limit).await?;
    Ok(Json(QobuzFavoritesListResponse { items, total }))
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
        favorites::upsert_album(&state.db, id, "", "").await?;
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
    use crate::services::download::{spawn_worker, WorkerDeps};

    pub async fn test_state() -> AppState {
        let library_path = std::env::temp_dir().join(format!(
            "euterpe-server-test-{}",
            std::process::id()
        ));
        let config = AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: Some(
                crate::crypto::MasterKey::parse(&hex::encode([1u8; 32])).unwrap(),
            ),
            qobuz_user_id: None,
            qobuz_auth_token: None,
            library_path,
            download_concurrency: 2,
        };
        let pool = db::connect(&config.database_url).await.unwrap();
        db::migrate(&pool).await.unwrap();

        let (job_tx, job_rx) = mpsc::channel(32);
        let (events, _) = broadcast::channel(16);

        let state = AppState::new(config.clone(), pool.clone(), job_tx, events.clone())
            .await
            .unwrap();

        spawn_worker(
            job_rx,
            WorkerDeps {
                pool,
                qobuz: Arc::clone(&state.qobuz),
                config: Arc::new(config),
                events,
                http: Client::new(),
            },
        );

        state
    }
}

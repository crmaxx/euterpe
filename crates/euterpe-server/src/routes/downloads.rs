use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::db::download_jobs::PriorityDirection;

use euterpe_qobuz::parse_album_url;

use crate::api::{
    CreateDownloadByUrlRequest, CreateDownloadRequest, CreateDownloadResponse,
    DownloadJobListResponse, DownloadJobStatus, DownloadJobType, DownloadPurgeResponse,
};
use crate::db::{download_jobs, favorites};
use crate::error::ApiError;
use crate::services::download::{
    format_album_display_title, quality_from_format_id, DownloadJobPayload,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListDownloadsQuery {
    pub status: Option<DownloadJobStatus>,
    #[serde(default = "default_download_limit")]
    pub limit: u32,
    #[serde(default = "default_download_sort")]
    pub sort: String,
    #[serde(default)]
    pub order: Option<String>,
    pub cursor: Option<String>,
}

fn default_download_limit() -> u32 {
    100
}

fn default_download_sort() -> String {
    "queue_position".to_string()
}

#[derive(Debug, Deserialize)]
pub struct DeleteDownloadQuery {
    /// When `1` or `true`, delete the job row (terminal jobs only).
    pub purge: Option<String>,
}

fn purge_requested(q: &DeleteDownloadQuery) -> bool {
    q.purge
        .as_deref()
        .is_some_and(|s| s == "1" || s.eq_ignore_ascii_case("true"))
}

async fn queue_album_download(
    state: &AppState,
    album_api_id: &str,
    quality: u8,
    qobuz_id: Option<u64>,
    display_title: Option<String>,
) -> Result<i64, ApiError> {
    let qobuz_for_dedup = qobuz_id.filter(|id| *id > 0);
    if download_jobs::has_running_album(&state.db, album_api_id, qobuz_for_dedup, quality).await? {
        return Err(ApiError::Message(
            "JOB_ALREADY_RUNNING: album download in progress".into(),
        ));
    }

    let payload = DownloadJobPayload {
        album_api_id: Some(album_api_id.to_string()),
        display_title: display_title.filter(|s| !s.trim().is_empty()),
        torrent: None,
    };
    let catalog_id = qobuz_id.unwrap_or(0);
    let job_id = download_jobs::insert_queued(
        &state.db,
        DownloadJobType::Album,
        catalog_id,
        quality,
        Some(&payload),
    )
    .await?;

    tracing::debug!(
        job_id,
        qobuz_id = ?qobuz_id,
        quality,
        album_api_id = %album_api_id,
        "download job queued"
    );

    state
        .job_tx
        .send(job_id)
        .await
        .map_err(|e| ApiError::Message(format!("job queue closed: {e}")))?;

    Ok(job_id)
}

pub async fn create_download(
    State(state): State<AppState>,
    Json(body): Json<CreateDownloadRequest>,
) -> Result<(StatusCode, Json<CreateDownloadResponse>), ApiError> {
    state.require_credentials().await?;

    if body.job_type != DownloadJobType::Album {
        return Err(ApiError::bad_request("only job_type=album is supported"));
    }

    let album_api_id = body.album_api_id.trim();
    if album_api_id.is_empty() {
        return Err(ApiError::bad_request(
            "album_api_id is required (Qobuz album/get id, e.g. zg7pv28g4mldg); use album_api_id from GET /api/v1/qobuz/favorites",
        ));
    }

    quality_from_format_id(body.quality)
        .ok_or_else(|| ApiError::bad_request("unsupported quality (use 5, 6, 7, or 27)"))?;

    let resolved_api_id = if let Some(catalog_id) = body.qobuz_id.filter(|id| *id > 0) {
        crate::services::download::resolve_album_api_id_for_state(
            &state,
            catalog_id,
            None,
        )
        .await?
        .unwrap_or_else(|| album_api_id.to_string())
    } else {
        album_api_id.to_string()
    };

    let display_title = if let Some(catalog_id) = body.qobuz_id.filter(|id| *id > 0) {
        favorites::album_meta(&state.db, catalog_id)
            .await?
            .map(|m| format_album_display_title(&m.artist_name, &m.title))
    } else {
        None
    };

    let job_id = queue_album_download(
        &state,
        &resolved_api_id,
        body.quality,
        body.qobuz_id,
        display_title,
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(CreateDownloadResponse { job_id }),
    ))
}

pub async fn create_download_by_url(
    State(state): State<AppState>,
    Json(body): Json<CreateDownloadByUrlRequest>,
) -> Result<(StatusCode, Json<CreateDownloadResponse>), ApiError> {
    state.require_credentials().await?;

    if body.url.trim().is_empty() {
        return Err(ApiError::bad_request("url must not be empty"));
    }

    quality_from_format_id(body.quality)
        .ok_or_else(|| ApiError::bad_request("unsupported quality (use 5, 6, 7, or 27)"))?;

    let album_ref = parse_album_url(&body.url).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let summary = {
        let guard = state.qobuz.lock().await;
        guard.album_ref(&album_ref).await?.summary
    };
    // Keep the same `album_id` that just succeeded in `album/get` (UPC / short ref).
    // `pick_album_api_id` may return a human slug that 404s on a second request.
    let album_api_id = album_ref;

    let artist = summary
        .artist
        .as_ref()
        .map(|a| a.name.as_str())
        .unwrap_or("");
    let display_title = Some(format_album_display_title(artist, &summary.title));

    let job_id = queue_album_download(
        &state,
        &album_api_id,
        body.quality,
        Some(summary.id),
        display_title,
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(CreateDownloadResponse { job_id }),
    ))
}

pub async fn list_downloads(
    State(state): State<AppState>,
    Query(q): Query<ListDownloadsQuery>,
) -> Result<Json<DownloadJobListResponse>, ApiError> {
    use crate::api::keyset::parse_limit;
    use crate::api::SortOrder;
    use crate::db::download_jobs::{DownloadsListParams, DownloadsSort};

    let limit = parse_limit(q.limit, 100, 500)?;
    let sort = DownloadsSort::parse(&q.sort)?;
    let order = match q.order.as_deref() {
        None => {
            if sort == DownloadsSort::QueuePosition {
                SortOrder::Asc
            } else {
                SortOrder::Desc
            }
        }
        Some(s) => SortOrder::parse(s)?,
    };
    let page = download_jobs::list_keyset(
        &state.db,
        DownloadsListParams {
            sort,
            order,
            limit,
            status: q.status,
            cursor: q.cursor,
        },
    )
    .await?;
    Ok(Json(DownloadJobListResponse {
        items: page.items,
        next_cursor: page.next_cursor,
        has_more: page.has_more,
    }))
}

pub async fn get_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<crate::api::DownloadJob>, ApiError> {
    download_jobs::get(&state.db, id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::Message(format!("job {id} not found")))
}

pub async fn purge_finished_downloads(
    State(state): State<AppState>,
) -> Result<Json<DownloadPurgeResponse>, ApiError> {
    let deleted = download_jobs::purge_finished(&state.db).await? as i64;
    Ok(Json(DownloadPurgeResponse { deleted }))
}

pub async fn delete_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<DeleteDownloadQuery>,
) -> Result<StatusCode, ApiError> {
    let job = download_jobs::get(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message(format!("job {id} not found")))?;

    if purge_requested(&q) {
        if !download_jobs::is_terminal_status(job.status) {
            return Err(ApiError::Message(
                "cannot purge queued or running job; cancel it first".into(),
            ));
        }
        if !download_jobs::delete_by_id(&state.db, id).await? {
            return Err(ApiError::Message(format!("job {id} not found")));
        }
        return Ok(StatusCode::NO_CONTENT);
    }

    if matches!(
        job.status,
        DownloadJobStatus::Completed | DownloadJobStatus::Failed
    ) {
        return Err(ApiError::Message(
            "cannot cancel completed or failed job".into(),
        ));
    }

    if job.job_type == crate::api::DownloadJobType::Torrent {
        if let Some(payload) = download_jobs::get_payload(&state.db, id).await? {
            if let Some(t) = payload.torrent {
                if let (Some(engine), Some(lid)) = (state.torrent.as_ref(), t.librqbit_id) {
                    let handle = euterpe_torrent::JobHandle {
                        librqbit_id: lid,
                        info_hash: t.info_hash,
                    };
                    let _ = engine.cancel(&handle).await;
                }
            }
        }
    }

    if !download_jobs::cancel(&state.db, id).await? {
        return Err(ApiError::Message(format!("job {id} not found")));
    }

    let _ = state.job_tx.send(0).await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn patch_download_priority(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<crate::api::PatchDownloadPriorityRequest>,
) -> Result<StatusCode, ApiError> {
    let direction = match body.direction.as_str() {
        "up" => PriorityDirection::Up,
        "down" => PriorityDirection::Down,
        _ => {
            return Err(ApiError::bad_request(
                "direction must be up or down",
            ));
        }
    };

    download_jobs::adjust_queue_priority(&state.db, id, direction).await?;
    let _ = state.job_tx.send(0).await;
    Ok(StatusCode::NO_CONTENT)
}

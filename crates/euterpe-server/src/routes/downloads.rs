use std::cmp::Ordering;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use euterpe_data::repositories::download_jobs::{
    self as data_download_jobs, DownloadJobRow, PriorityDirection,
};
use euterpe_data::repositories::favorites;

use euterpe_qobuz::parse_album_url;

use crate::api::keyset::{
    decode_cursor, ensure_cursor_matches, fingerprint_json, finish_keyset_page,
};
use crate::api::{
    CreateDownloadByUrlRequest, CreateDownloadRequest, CreateDownloadResponse, DownloadJob,
    DownloadJobListResponse, DownloadJobStatus, DownloadJobType, DownloadPurgeResponse,
    DownloadRetryResponse, SortKeyKind, SortKeyValue,
};
use crate::error::ApiError;
use crate::services::download::{
    DownloadJobPayload, format_album_display_title, quality_from_format_id,
};
use crate::services::torrent_cleanup;
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

/// Tolerate corrupt or legacy `payload_json` so one bad row does not break `GET /downloads`.
fn parse_job_payload(job_id: i64, raw: Option<&str>) -> DownloadJobPayload {
    let Some(raw) = raw else {
        return DownloadJobPayload::default();
    };
    match serde_json::from_str(raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(
                job_id,
                error = %e,
                "download job payload JSON invalid; listing job with empty payload"
            );
            DownloadJobPayload::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DownloadsSort {
    Id,
    CreatedAt,
    Status,
    QueuePosition,
}

impl DownloadsSort {
    fn parse(s: &str) -> Result<Self, ApiError> {
        match s {
            "id" => Ok(Self::Id),
            "created_at" => Ok(Self::CreatedAt),
            "status" => Ok(Self::Status),
            "queue_position" => Ok(Self::QueuePosition),
            _ => Err(ApiError::bad_request(
                "sort must be id, created_at, status, or queue_position",
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::CreatedAt => "created_at",
            Self::Status => "status",
            Self::QueuePosition => "queue_position",
        }
    }

    fn key_kind(self) -> SortKeyKind {
        match self {
            Self::Id | Self::QueuePosition => SortKeyKind::Int,
            _ => SortKeyKind::Text,
        }
    }

    fn primary_key(self, row: &DownloadJobRow) -> SortKeyValue {
        match self {
            Self::Id => SortKeyValue::Int(row.id),
            Self::CreatedAt => SortKeyValue::Text(row.created_at.clone()),
            Self::Status => SortKeyValue::Text(row.status.as_str().to_string()),
            Self::QueuePosition => SortKeyValue::Int(row.queue_position),
        }
    }
}

fn api_status_to_data(
    status: DownloadJobStatus,
) -> euterpe_data::repositories::download_jobs::DownloadJobStatus {
    match status {
        DownloadJobStatus::Queued => data_download_jobs::DownloadJobStatus::Queued,
        DownloadJobStatus::Running => data_download_jobs::DownloadJobStatus::Running,
        DownloadJobStatus::Paused => data_download_jobs::DownloadJobStatus::Paused,
        DownloadJobStatus::Completed => data_download_jobs::DownloadJobStatus::Completed,
        DownloadJobStatus::Failed => data_download_jobs::DownloadJobStatus::Failed,
        DownloadJobStatus::Cancelled => data_download_jobs::DownloadJobStatus::Cancelled,
    }
}

fn data_status_to_api(
    status: euterpe_data::repositories::download_jobs::DownloadJobStatus,
) -> DownloadJobStatus {
    match status {
        data_download_jobs::DownloadJobStatus::Queued => DownloadJobStatus::Queued,
        data_download_jobs::DownloadJobStatus::Running => DownloadJobStatus::Running,
        data_download_jobs::DownloadJobStatus::Paused => DownloadJobStatus::Paused,
        data_download_jobs::DownloadJobStatus::Completed => DownloadJobStatus::Completed,
        data_download_jobs::DownloadJobStatus::Failed => DownloadJobStatus::Failed,
        data_download_jobs::DownloadJobStatus::Cancelled => DownloadJobStatus::Cancelled,
    }
}

fn data_type_to_api(
    job_type: euterpe_data::repositories::download_jobs::DownloadJobType,
) -> DownloadJobType {
    match job_type {
        data_download_jobs::DownloadJobType::Album => DownloadJobType::Album,
        data_download_jobs::DownloadJobType::Track => DownloadJobType::Track,
        data_download_jobs::DownloadJobType::Artist => DownloadJobType::Artist,
        data_download_jobs::DownloadJobType::Playlist => DownloadJobType::Playlist,
        data_download_jobs::DownloadJobType::Torrent => DownloadJobType::Torrent,
    }
}

fn row_into_api_job(row: DownloadJobRow) -> DownloadJob {
    let job_type = data_type_to_api(row.job_type);
    let payload = parse_job_payload(row.id, row.payload_json.as_deref());
    DownloadJob {
        id: row.id,
        status: data_status_to_api(row.status),
        job_type,
        source: payload.source(job_type),
        display_title: payload.display_title(job_type),
        qobuz_id: row.qobuz_id.unwrap_or(0),
        quality: row.quality,
        progress_pct: row.progress_pct,
        download_speed_bps: row.download_speed_bps.max(0) as u64,
        queue_position: row.queue_position,
        torrent_detail: payload.torrent_detail_for_api(),
        error_message: row.error_message,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn compare_job_rows(
    left: &DownloadJobRow,
    right: &DownloadJobRow,
    sort: DownloadsSort,
    order: crate::api::SortOrder,
) -> Ordering {
    let primary = compare_sort_values(&sort.primary_key(left), &sort.primary_key(right));
    let ordered = match order {
        crate::api::SortOrder::Asc => primary,
        crate::api::SortOrder::Desc => primary.reverse(),
    };
    ordered.then_with(|| left.id.cmp(&right.id))
}

fn row_is_after_cursor(
    row: &DownloadJobRow,
    sort: DownloadsSort,
    order: crate::api::SortOrder,
    primary: &SortKeyValue,
    tie_id: i64,
) -> bool {
    let row_primary = sort.primary_key(row);
    match compare_sort_values(&row_primary, primary) {
        Ordering::Greater => order == crate::api::SortOrder::Asc,
        Ordering::Less => order == crate::api::SortOrder::Desc,
        Ordering::Equal => row.id > tie_id,
    }
}

fn compare_sort_values(left: &SortKeyValue, right: &SortKeyValue) -> Ordering {
    match (left, right) {
        (SortKeyValue::Text(left), SortKeyValue::Text(right)) => left.cmp(right),
        (SortKeyValue::Int(left), SortKeyValue::Int(right)) => left.cmp(right),
        (SortKeyValue::Bool(left), SortKeyValue::Bool(right)) => left.cmp(right),
        _ => Ordering::Equal,
    }
}

async fn cancel_torrent_for_job(state: &AppState, id: i64) -> Result<(), ApiError> {
    let Some(payload) =
        data_download_jobs::get_payload::<DownloadJobPayload>(&state.data, id).await?
    else {
        return Ok(());
    };
    let Some(t) = payload.torrent else {
        return Ok(());
    };
    let (Some(engine), Some(lid)) = (state.torrent.as_ref(), t.librqbit_id) else {
        return Ok(());
    };
    let handle = euterpe_torrent::JobHandle {
        librqbit_id: lid,
        info_hash: t.info_hash,
    };
    let _ = engine.cancel(&handle).await;
    Ok(())
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
        crate::services::download::resolve_album_api_id_for_state(&state, catalog_id, None)
            .await?
            .unwrap_or_else(|| album_api_id.to_string())
    } else {
        album_api_id.to_string()
    };

    let display_title = if let Some(catalog_id) = body.qobuz_id.filter(|id| *id > 0) {
        favorites::album_meta(&state.data, catalog_id)
            .await?
            .map(|m| format_album_display_title(&m.artist_name, &m.title))
    } else {
        None
    };

    let job_id = crate::services::download::queue_album_download_for_state(
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

    let job_id = crate::services::download::queue_album_download_for_state(
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
    use crate::api::SortOrder;
    use crate::api::keyset::parse_limit;

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
    let fingerprint = fingerprint_json(&json!({
        "status": q.status.map(|s| s.as_str()),
    }));

    let mut after: Option<(SortKeyValue, i64)> = None;
    if let Some(ref cursor_str) = q.cursor {
        let payload = decode_cursor(cursor_str)?;
        let (primary, tie) = ensure_cursor_matches(
            &payload,
            sort.as_str(),
            order,
            &fingerprint,
            sort.key_kind(),
        )?;
        after = Some((primary, tie));
    }

    let status = q.status.map(api_status_to_data);
    let mut rows: Vec<DownloadJobRow> = data_download_jobs::list(&state.data)
        .await?
        .into_iter()
        .filter(|row| status.is_none_or(|status| row.status == status))
        .collect();
    rows.sort_by(|left, right| compare_job_rows(left, right, sort, order));
    if let Some((primary, tie)) = after.as_ref() {
        rows.retain(|row| row_is_after_cursor(row, sort, order, primary, *tie));
    }
    rows.truncate(limit as usize + 1);
    let page = finish_keyset_page(
        rows,
        limit as usize,
        sort.as_str(),
        order,
        &fingerprint,
        |row| (sort.primary_key(row), row.id),
    );
    Ok(Json(DownloadJobListResponse {
        items: page.items.into_iter().map(row_into_api_job).collect(),
        next_cursor: page.next_cursor,
        has_more: page.has_more,
    }))
}

pub async fn get_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<crate::api::DownloadJob>, ApiError> {
    data_download_jobs::get_by_id(&state.data, id)
        .await?
        .map(row_into_api_job)
        .map(Json)
        .ok_or_else(|| ApiError::Message(format!("job {id} not found")))
}

pub async fn purge_completed_downloads(
    State(state): State<AppState>,
) -> Result<Json<DownloadPurgeResponse>, ApiError> {
    let torrent_ids = data_download_jobs::list_completed_torrent_job_ids(&state.data).await?;
    for id in torrent_ids {
        torrent_cleanup::remove_job_incoming_dir(&state, id).await?;
    }
    let deleted = data_download_jobs::purge_completed(&state.data).await? as i64;
    Ok(Json(DownloadPurgeResponse { deleted }))
}

pub async fn delete_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<DeleteDownloadQuery>,
) -> Result<StatusCode, ApiError> {
    let job = data_download_jobs::get_by_id(&state.data, id)
        .await?
        .ok_or_else(|| ApiError::Message(format!("job {id} not found")))?;

    if purge_requested(&q) {
        if !data_download_jobs::is_terminal_status(job.status) {
            return Err(ApiError::Message(
                "cannot purge active job; cancel it first".into(),
            ));
        }
        if job.job_type == data_download_jobs::DownloadJobType::Torrent {
            torrent_cleanup::remove_job_incoming_dir(&state, id).await?;
        }
        if !data_download_jobs::delete_by_id(&state.data, id).await? {
            return Err(ApiError::Message(format!("job {id} not found")));
        }
        return Ok(StatusCode::NO_CONTENT);
    }

    if matches!(
        data_status_to_api(job.status),
        DownloadJobStatus::Completed | DownloadJobStatus::Failed
    ) {
        return Err(ApiError::Message(
            "cannot cancel completed or failed job".into(),
        ));
    }

    cancel_torrent_for_job(&state, id).await?;

    if !data_download_jobs::cancel(&state.data, id).await? {
        return Err(ApiError::Message(format!("job {id} not found")));
    }

    if job.job_type == data_download_jobs::DownloadJobType::Torrent {
        torrent_cleanup::remove_job_incoming_dir(&state, id).await?;
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
            return Err(ApiError::bad_request("direction must be up or down"));
        }
    };

    data_download_jobs::adjust_queue_priority(&state.data, id, direction).await?;
    let _ = state.job_tx.send(0).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn retry_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    data_download_jobs::retry_failed(&state.data, id).await?;
    let _ = state.job_tx.send(0).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn retry_failed_downloads(
    State(state): State<AppState>,
) -> Result<Json<DownloadRetryResponse>, ApiError> {
    let retried = data_download_jobs::retry_all_failed(&state.data).await? as i64;
    let _ = state.job_tx.send(0).await;
    Ok(Json(DownloadRetryResponse { retried }))
}

pub async fn pause_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let job = data_download_jobs::get_by_id(&state.data, id)
        .await?
        .ok_or_else(|| ApiError::Message(format!("job {id} not found")))?;

    data_download_jobs::pause(&state.data, id).await?;

    if job.job_type == data_download_jobs::DownloadJobType::Torrent {
        cancel_torrent_for_job(&state, id).await?;
    }

    let _ = state.job_tx.send(0).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn resume_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    data_download_jobs::resume_paused(&state.data, id).await?;
    let _ = state.job_tx.send(0).await;
    Ok(StatusCode::NO_CONTENT)
}

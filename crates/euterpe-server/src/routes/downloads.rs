use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::api::{
    CreateDownloadRequest, CreateDownloadResponse, DownloadJobListResponse, DownloadJobStatus,
    DownloadJobType,
};
use crate::db::download_jobs;
use crate::error::ApiError;
use crate::services::download::{quality_from_format_id, DownloadJobPayload};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListDownloadsQuery {
    pub status: Option<DownloadJobStatus>,
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

    let qobuz_for_dedup = body.qobuz_id.filter(|id| *id > 0);
    if download_jobs::has_running_album(
        &state.db,
        album_api_id,
        qobuz_for_dedup,
        body.quality,
    )
    .await?
    {
        return Err(ApiError::Message(
            "JOB_ALREADY_RUNNING: album download in progress".into(),
        ));
    }

    let payload = DownloadJobPayload {
        album_api_id: Some(album_api_id.to_string()),
    };
    let qobuz_id = body.qobuz_id.unwrap_or(0);
    let job_id = download_jobs::insert_queued(
        &state.db,
        body.job_type,
        qobuz_id,
        body.quality,
        Some(&payload),
    )
    .await?;

    tracing::debug!(
        job_id,
        qobuz_id = body.qobuz_id,
        quality = body.quality,
        album_api_id = %album_api_id,
        "download job queued"
    );

    state
        .job_tx
        .send(job_id)
        .await
        .map_err(|e| ApiError::Message(format!("job queue closed: {e}")))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(CreateDownloadResponse { job_id }),
    ))
}

pub async fn list_downloads(
    State(state): State<AppState>,
    Query(q): Query<ListDownloadsQuery>,
) -> Result<Json<DownloadJobListResponse>, ApiError> {
    let items = download_jobs::list(&state.db, q.status).await?;
    Ok(Json(DownloadJobListResponse { items }))
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

pub async fn cancel_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let job = download_jobs::get(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message(format!("job {id} not found")))?;

    if matches!(
        job.status,
        DownloadJobStatus::Completed | DownloadJobStatus::Failed
    ) {
        return Err(ApiError::Message(
            "cannot cancel completed or failed job".into(),
        ));
    }

    if !download_jobs::cancel(&state.db, id).await? {
        return Err(ApiError::Message(format!("job {id} not found")));
    }

    Ok(StatusCode::NO_CONTENT)
}

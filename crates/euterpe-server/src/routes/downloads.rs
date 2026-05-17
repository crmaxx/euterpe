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
use crate::services::download::quality_from_format_id;
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

    quality_from_format_id(body.quality)
        .ok_or_else(|| ApiError::bad_request("unsupported quality (use 5, 6, 7, or 27)"))?;

    if download_jobs::has_running_album(&state.db, body.qobuz_id, body.quality).await? {
        return Err(ApiError::Message(
            "JOB_ALREADY_RUNNING: album download in progress".into(),
        ));
    }

    let job_id =
        download_jobs::insert_queued(&state.db, body.job_type, body.qobuz_id, body.quality)
            .await?;

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

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::cue_jobs as data;

#[derive(Debug, Clone)]
pub struct CueJobRow {
    pub id: i64,
    pub album_id: i64,
    pub status: String,
    pub tracks_total: i64,
    pub tracks_done: i64,
    pub progress_pct: f64,
    pub error_message: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CueJobPayload {
    pub cue_path: String,
    pub audio_path: String,
    pub source_file_policy: String,
}

pub async fn create_queued(
    pool: &SqlitePool,
    album_id: i64,
    tracks_total: i64,
    payload_json: Option<&str>,
) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    let payload = payload_json
        .map(serde_json::from_str::<CueJobPayload>)
        .transpose()
        .map_err(|e| ApiError::Message(e.to_string()))?;
    let payload = payload.map(cue_payload_to_data);
    Ok(data::create_queued(&handle, album_id, tracks_total, payload.as_ref()).await?)
}

pub async fn latest_for_album(
    pool: &SqlitePool,
    album_id: i64,
) -> Result<Option<CueJobRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::latest_for_album(&handle, album_id)
        .await?
        .map(cue_row_from_data))
}

pub async fn mark_running(pool: &SqlitePool, job_id: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::mark_running(&handle, job_id).await?;
    Ok(())
}

pub async fn finish_success(
    pool: &SqlitePool,
    job_id: i64,
    tracks_done: i64,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::finish_success(&handle, job_id, tracks_done).await?;
    Ok(())
}

pub async fn update_progress(
    pool: &SqlitePool,
    job_id: i64,
    tracks_done: i64,
    tracks_total: i64,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::update_progress(&handle, job_id, tracks_done, tracks_total).await?;
    Ok(())
}

pub async fn finish_failed(pool: &SqlitePool, job_id: i64, error: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::finish_failed(&handle, job_id, error).await?;
    Ok(())
}

pub fn row_to_summary(row: CueJobRow) -> crate::api::CueJobSummary {
    crate::api::CueJobSummary {
        id: row.id,
        album_id: row.album_id,
        status: row.status,
        tracks_total: row.tracks_total,
        tracks_done: row.tracks_done,
        progress_pct: row.progress_pct,
        error_message: row.error_message,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn cue_row_from_data(row: data::CueJobRow) -> CueJobRow {
    CueJobRow {
        id: row.id,
        album_id: row.album_id,
        status: row.status.as_str().to_string(),
        tracks_total: row.tracks_total,
        tracks_done: row.tracks_done,
        progress_pct: row.progress_pct,
        error_message: row.error_message,
        payload_json: row.payload_json,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn cue_payload_to_data(payload: CueJobPayload) -> data::CueJobPayload {
    data::CueJobPayload {
        cue_path: payload.cue_path,
        audio_path: payload.audio_path,
        source_file_policy: payload.source_file_policy,
    }
}

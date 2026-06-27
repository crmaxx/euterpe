use serde::{Deserialize, Serialize};
use welds::WeldsModel;

use crate::connection::DataHandle;
use crate::error::{DataError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueJobStatus {
    Queued,
    Running,
    Success,
    Failed,
}

impl CueJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueJobRow {
    pub id: i64,
    pub album_id: i64,
    pub status: CueJobStatus,
    pub tracks_total: i64,
    pub tracks_done: i64,
    pub progress_pct: f64,
    pub error_message: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CueJobPayload {
    pub cue_path: String,
    pub audio_path: String,
    pub source_file_policy: String,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "cue_jobs")]
struct CueJob {
    #[welds(primary_key)]
    id: i64,
    album_id: i64,
    status: String,
    tracks_total: i64,
    tracks_done: i64,
    progress_pct: f64,
    error_message: Option<String>,
    payload_json: Option<String>,
    created_at: String,
    updated_at: String,
}

pub async fn create_queued(
    handle: &DataHandle,
    album_id: i64,
    tracks_total: i64,
    payload: Option<&CueJobPayload>,
) -> Result<i64> {
    if album_has_active_job(handle, album_id).await? {
        return Err(DataError::Config(
            "active CUE job already exists for album".to_string(),
        ));
    }
    let now = sqlite_timestamp();
    let mut job = CueJob::new();
    job.album_id = album_id;
    job.status = CueJobStatus::Queued.as_str().to_string();
    job.tracks_total = tracks_total;
    job.tracks_done = 0;
    job.progress_pct = 0.0;
    job.error_message = None;
    job.payload_json = payload.map(serde_json::to_string).transpose()?;
    job.created_at = now.clone();
    job.updated_at = now;
    job.save(handle.client()).await?;
    Ok(job.id)
}

pub async fn album_has_active_job(handle: &DataHandle, album_id: i64) -> Result<bool> {
    Ok(CueJob::all().run(handle.client()).await?.iter().any(|job| {
        job.album_id == album_id
            && CueJobStatus::parse(&job.status).is_some_and(CueJobStatus::is_active)
    }))
}

pub async fn get_by_id(handle: &DataHandle, id: i64) -> Result<Option<CueJobRow>> {
    CueJob::find_by_id(handle.client(), id)
        .await?
        .map(row_from_state)
        .transpose()
}

pub async fn latest_for_album(handle: &DataHandle, album_id: i64) -> Result<Option<CueJobRow>> {
    CueJob::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|job| job.album_id == album_id)
        .max_by_key(|job| job.id)
        .map(row_from_state)
        .transpose()
}

pub async fn mark_running(handle: &DataHandle, job_id: i64) -> Result<()> {
    if let Some(mut job) = CueJob::find_by_id(handle.client(), job_id).await? {
        job.status = CueJobStatus::Running.as_str().to_string();
        job.updated_at = sqlite_timestamp();
        job.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn finish_success(handle: &DataHandle, job_id: i64, tracks_done: i64) -> Result<()> {
    if let Some(mut job) = CueJob::find_by_id(handle.client(), job_id).await? {
        job.status = CueJobStatus::Success.as_str().to_string();
        job.tracks_done = tracks_done;
        job.progress_pct = 100.0;
        job.error_message = None;
        job.updated_at = sqlite_timestamp();
        job.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn update_progress(
    handle: &DataHandle,
    job_id: i64,
    tracks_done: i64,
    tracks_total: i64,
) -> Result<()> {
    let Some(mut job) = CueJob::find_by_id(handle.client(), job_id).await? else {
        return Ok(());
    };
    if job.status != CueJobStatus::Running.as_str() {
        return Ok(());
    }
    job.tracks_done = tracks_done;
    job.tracks_total = tracks_total;
    job.progress_pct = if tracks_total > 0 {
        (tracks_done as f64 / tracks_total as f64) * 100.0
    } else {
        0.0
    };
    job.updated_at = sqlite_timestamp();
    job.save(handle.client()).await?;
    Ok(())
}

pub async fn finish_failed(handle: &DataHandle, job_id: i64, error: &str) -> Result<()> {
    if let Some(mut job) = CueJob::find_by_id(handle.client(), job_id).await? {
        job.status = CueJobStatus::Failed.as_str().to_string();
        job.error_message = Some(error.to_string());
        job.updated_at = sqlite_timestamp();
        job.save(handle.client()).await?;
    }
    Ok(())
}

fn row_from_state(row: welds::state::DbState<CueJob>) -> Result<CueJobRow> {
    let model = row.into_inner();
    Ok(CueJobRow {
        id: model.id,
        album_id: model.album_id,
        status: parse_status(&model.status)?,
        tracks_total: model.tracks_total,
        tracks_done: model.tracks_done,
        progress_pct: model.progress_pct,
        error_message: model.error_message,
        payload_json: model.payload_json,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

fn parse_status(value: &str) -> Result<CueJobStatus> {
    CueJobStatus::parse(value)
        .ok_or_else(|| DataError::Config(format!("invalid CUE job status {value}")))
}

fn sqlite_timestamp() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

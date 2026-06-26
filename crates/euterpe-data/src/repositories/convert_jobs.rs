use welds::prelude::*;

use crate::connection::DataHandle;
use crate::error::{DataError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertJobStatus {
    Queued,
    Running,
    Success,
    Failed,
    Cancelled,
}

impl ConvertJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertTrigger {
    Manual,
    Auto,
}

impl ConvertTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(Self::Manual),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConvertJobRow {
    pub id: i64,
    pub album_id: i64,
    pub status: ConvertJobStatus,
    pub trigger: ConvertTrigger,
    pub files_total: i64,
    pub files_done: i64,
    pub progress_pct: f64,
    pub error_message: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "convert_jobs")]
struct ConvertJob {
    #[welds(primary_key)]
    id: i64,
    album_id: i64,
    status: String,
    trigger: String,
    files_total: i64,
    files_done: i64,
    progress_pct: f64,
    error_message: Option<String>,
    payload_json: Option<String>,
    created_at: String,
    updated_at: String,
}

pub async fn album_has_active_job(handle: &DataHandle, album_id: i64) -> Result<bool> {
    Ok(ConvertJob::all()
        .run(handle.client())
        .await?
        .iter()
        .any(|job| {
            job.album_id == album_id
                && ConvertJobStatus::parse(&job.status).is_some_and(ConvertJobStatus::is_active)
        }))
}

pub async fn create(
    handle: &DataHandle,
    album_id: i64,
    trigger: ConvertTrigger,
    files_total: i64,
) -> Result<i64> {
    if album_has_active_job(handle, album_id).await? {
        return Err(DataError::InvalidOperation(
            "active convert job already exists for album".to_string(),
        ));
    }
    let now = sqlite_timestamp();
    let mut job = ConvertJob::new();
    job.album_id = album_id;
    job.status = ConvertJobStatus::Queued.as_str().to_string();
    job.trigger = trigger.as_str().to_string();
    job.files_total = files_total;
    job.files_done = 0;
    job.progress_pct = 0.0;
    job.error_message = None;
    job.payload_json = None;
    job.created_at = now.clone();
    job.updated_at = now;
    job.save(handle.client()).await?;
    Ok(job.id)
}

pub async fn enqueue_album_if_needed(
    handle: &DataHandle,
    album_id: i64,
    files_total: i64,
) -> Result<Option<i64>> {
    if album_has_active_job(handle, album_id).await? {
        return Ok(None);
    }
    create(handle, album_id, ConvertTrigger::Auto, files_total)
        .await
        .map(Some)
}

pub async fn claim_running(handle: &DataHandle, id: i64) -> Result<bool> {
    let updated = ConvertJob::where_col(|job| job.id.equal(id))
        .where_col(|job| job.status.equal(ConvertJobStatus::Queued.as_str()))
        .set(
            |job| job.status,
            ConvertJobStatus::Running.as_str().to_string(),
        )
        .set(|job| job.updated_at, sqlite_timestamp())
        .run(handle.client())
        .await?;
    Ok(updated == 1)
}

pub async fn next_queued_id(handle: &DataHandle) -> Result<Option<i64>> {
    Ok(ConvertJob::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|job| job.status == ConvertJobStatus::Queued.as_str())
        .min_by_key(|job| job.id)
        .map(|job| job.id))
}

pub async fn update_progress(
    handle: &DataHandle,
    id: i64,
    files_done: i64,
    files_total: i64,
    progress_pct: f64,
    payload_json: Option<&str>,
) -> Result<bool> {
    let Some(mut job) = ConvertJob::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    if job.status != ConvertJobStatus::Running.as_str() {
        return Ok(false);
    }
    job.files_done = files_done;
    job.files_total = files_total;
    job.progress_pct = progress_pct;
    if let Some(payload_json) = payload_json {
        job.payload_json = Some(payload_json.to_string());
    }
    job.updated_at = sqlite_timestamp();
    job.save(handle.client()).await?;
    Ok(true)
}

pub async fn finish(
    handle: &DataHandle,
    id: i64,
    status: ConvertJobStatus,
    error_message: Option<&str>,
    payload_json: Option<&str>,
) -> Result<()> {
    if let Some(mut job) = ConvertJob::find_by_id(handle.client(), id).await? {
        job.status = status.as_str().to_string();
        job.error_message = error_message.map(ToString::to_string);
        if let Some(payload_json) = payload_json {
            job.payload_json = Some(payload_json.to_string());
        }
        if status == ConvertJobStatus::Success {
            job.progress_pct = 100.0;
        }
        job.updated_at = sqlite_timestamp();
        job.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn get_by_id(handle: &DataHandle, id: i64) -> Result<Option<ConvertJobRow>> {
    ConvertJob::find_by_id(handle.client(), id)
        .await?
        .map(row_from_state)
        .transpose()
}

pub async fn latest_for_album(handle: &DataHandle, album_id: i64) -> Result<Option<ConvertJobRow>> {
    ConvertJob::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|job| job.album_id == album_id)
        .max_by_key(|job| job.id)
        .map(row_from_state)
        .transpose()
}

fn row_from_state(row: welds::state::DbState<ConvertJob>) -> Result<ConvertJobRow> {
    let model = row.into_inner();
    Ok(ConvertJobRow {
        id: model.id,
        album_id: model.album_id,
        status: parse_status(&model.status)?,
        trigger: parse_trigger(&model.trigger)?,
        files_total: model.files_total,
        files_done: model.files_done,
        progress_pct: model.progress_pct,
        error_message: model.error_message,
        payload_json: model.payload_json,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

fn parse_status(value: &str) -> Result<ConvertJobStatus> {
    ConvertJobStatus::parse(value)
        .ok_or_else(|| DataError::Config(format!("invalid convert job status {value}")))
}

fn parse_trigger(value: &str) -> Result<ConvertTrigger> {
    ConvertTrigger::parse(value)
        .ok_or_else(|| DataError::Config(format!("invalid convert job trigger {value}")))
}

fn sqlite_timestamp() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

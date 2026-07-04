use std::collections::HashMap;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use welds::prelude::*;

use crate::connection::DataHandle;
use crate::error::{DataError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadJobStatus {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DownloadJobType {
    Album,
    Track,
    Artist,
    Playlist,
    Torrent,
}

impl DownloadJobType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Album => "album",
            Self::Track => "track",
            Self::Artist => "artist",
            Self::Playlist => "playlist",
            Self::Torrent => "torrent",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "album" => Some(Self::Album),
            "track" => Some(Self::Track),
            "artist" => Some(Self::Artist),
            "playlist" => Some(Self::Playlist),
            "torrent" => Some(Self::Torrent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadJobRow {
    pub id: i64,
    pub status: DownloadJobStatus,
    pub job_type: DownloadJobType,
    pub qobuz_id: Option<i64>,
    pub quality: i32,
    pub progress_pct: f64,
    pub download_speed_bps: i64,
    pub queue_position: i64,
    pub payload_json: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "download_jobs")]
struct DownloadJob {
    #[welds(primary_key)]
    id: i64,
    status: String,
    job_type: String,
    qobuz_id: Option<i64>,
    quality: i32,
    progress_pct: f64,
    download_speed_bps: i64,
    queue_position: i64,
    payload_json: Option<String>,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

pub fn can_transition(from: DownloadJobStatus, to: DownloadJobStatus) -> bool {
    use DownloadJobStatus::*;
    matches!(
        (from, to),
        (Queued, Running)
            | (Queued, Cancelled)
            | (Queued, Paused)
            | (Running, Completed)
            | (Running, Failed)
            | (Running, Cancelled)
            | (Running, Paused)
            | (Paused, Queued)
            | (Paused, Cancelled)
    )
}

pub fn is_terminal_status(status: DownloadJobStatus) -> bool {
    matches!(
        status,
        DownloadJobStatus::Completed | DownloadJobStatus::Failed | DownloadJobStatus::Cancelled
    )
}

pub async fn next_queue_position(handle: &DataHandle, job_type: DownloadJobType) -> Result<i64> {
    let max_position = DownloadJob::all()
        .run(handle.client())
        .await?
        .iter()
        .filter(|job| {
            job.job_type == job_type.as_str() && job.status == DownloadJobStatus::Queued.as_str()
        })
        .map(|job| job.queue_position)
        .max()
        .unwrap_or(0);
    Ok(max_position + 1)
}

pub async fn insert_queued<T: Serialize + ?Sized>(
    handle: &DataHandle,
    job_type: DownloadJobType,
    qobuz_id: Option<i64>,
    quality: u8,
    payload: Option<&T>,
) -> Result<i64> {
    let payload_json = payload.map(serde_json::to_string).transpose()?;
    let now = sqlite_timestamp();
    let mut job = DownloadJob::new();
    job.status = DownloadJobStatus::Queued.as_str().to_string();
    job.job_type = job_type.as_str().to_string();
    job.qobuz_id = qobuz_id;
    job.quality = quality as i32;
    job.progress_pct = 0.0;
    job.download_speed_bps = 0;
    job.queue_position = next_queue_position(handle, job_type).await?;
    job.payload_json = payload_json;
    job.error_message = None;
    job.created_at = now.clone();
    job.updated_at = now;
    job.save(handle.client()).await?;
    Ok(job.id)
}

pub async fn set_payload<T: Serialize + ?Sized>(
    handle: &DataHandle,
    id: i64,
    payload: &T,
) -> Result<()> {
    if let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await? {
        job.payload_json = Some(serde_json::to_string(payload)?);
        job.updated_at = sqlite_timestamp();
        job.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn get_payload<T>(handle: &DataHandle, id: i64) -> Result<Option<T>>
where
    T: DeserializeOwned + Default,
{
    let Some(job) = DownloadJob::find_by_id(handle.client(), id).await? else {
        return Ok(None);
    };
    let Some(json) = job.payload_json.as_ref() else {
        return Ok(Some(T::default()));
    };
    Ok(Some(serde_json::from_str(json)?))
}

pub async fn get_by_id(handle: &DataHandle, id: i64) -> Result<Option<DownloadJobRow>> {
    DownloadJob::find_by_id(handle.client(), id)
        .await?
        .map(row_from_state)
        .transpose()
}

pub async fn list(handle: &DataHandle) -> Result<Vec<DownloadJobRow>> {
    DownloadJob::all()
        .run(handle.client())
        .await?
        .into_iter()
        .map(row_from_state)
        .collect()
}

pub async fn has_running_album(
    handle: &DataHandle,
    album_api_id: &str,
    qobuz_id: Option<u64>,
    quality: u8,
) -> Result<bool> {
    has_album_job_matching_status(
        handle,
        album_api_id,
        qobuz_id,
        quality,
        &[DownloadJobStatus::Running],
    )
    .await
}

pub async fn has_active_album(
    handle: &DataHandle,
    album_api_id: &str,
    qobuz_id: Option<u64>,
    quality: u8,
) -> Result<bool> {
    has_album_job_matching_status(
        handle,
        album_api_id,
        qobuz_id,
        quality,
        &[
            DownloadJobStatus::Queued,
            DownloadJobStatus::Running,
            DownloadJobStatus::Paused,
        ],
    )
    .await
}

async fn has_album_job_matching_status(
    handle: &DataHandle,
    album_api_id: &str,
    qobuz_id: Option<u64>,
    quality: u8,
    statuses: &[DownloadJobStatus],
) -> Result<bool> {
    for job in DownloadJob::all().run(handle.client()).await? {
        if !statuses.iter().any(|status| job.status == status.as_str())
            || job.job_type != DownloadJobType::Album.as_str()
            || job.quality != quality as i32
        {
            continue;
        }
        if qobuz_id.is_some_and(|id| job.qobuz_id == Some(id as i64)) {
            return Ok(true);
        }
        let Some(payload_json) = job.payload_json.as_ref() else {
            continue;
        };
        let payload: Value = serde_json::from_str(payload_json)?;
        if payload
            .get("album_api_id")
            .and_then(Value::as_str)
            .is_some_and(|value| value == album_api_id)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn count_running_by_type(handle: &DataHandle, job_type: DownloadJobType) -> Result<u64> {
    Ok(DownloadJob::all()
        .run(handle.client())
        .await?
        .iter()
        .filter(|job| {
            job.status == DownloadJobStatus::Running.as_str() && job.job_type == job_type.as_str()
        })
        .count() as u64)
}

pub async fn next_queued_id(handle: &DataHandle, job_type: DownloadJobType) -> Result<Option<i64>> {
    Ok(DownloadJob::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|job| {
            job.status == DownloadJobStatus::Queued.as_str() && job.job_type == job_type.as_str()
        })
        .min_by_key(|job| (job.queue_position, job.id))
        .map(|job| job.id))
}

pub async fn adjust_queue_priority(
    handle: &DataHandle,
    id: i64,
    direction: PriorityDirection,
) -> Result<()> {
    let tx = handle
        .client()
        .begin()
        .await
        .map_err(welds::WeldsError::from)?;
    let mut jobs = DownloadJob::all().run(&tx).await?;
    let Some(current_index) = jobs.iter().position(|job| job.id == id) else {
        return Err(DataError::InvalidOperation(format!("job {id} not found")));
    };
    if jobs[current_index].status != DownloadJobStatus::Queued.as_str() {
        return Err(DataError::InvalidOperation(
            "only queued jobs can be reordered".to_string(),
        ));
    }

    let job_type = jobs[current_index].job_type.clone();
    let mut queued = jobs
        .iter()
        .enumerate()
        .filter(|(_, job)| {
            job.status == DownloadJobStatus::Queued.as_str() && job.job_type == job_type
        })
        .map(|(index, job)| (index, job.queue_position, job.id))
        .collect::<Vec<_>>();
    queued.sort_by_key(|(_, queue_position, id)| (*queue_position, *id));
    let Some(order_index) = queued.iter().position(|(_, _, job_id)| *job_id == id) else {
        return Ok(());
    };
    let neighbor_order_index = match direction {
        PriorityDirection::Up if order_index > 0 => Some(order_index - 1),
        PriorityDirection::Down if order_index + 1 < queued.len() => Some(order_index + 1),
        _ => None,
    };
    let Some(neighbor_order_index) = neighbor_order_index else {
        return Ok(());
    };

    let neighbor_index = queued[neighbor_order_index].0;
    let current_position = jobs[current_index].queue_position;
    let neighbor_position = jobs[neighbor_index].queue_position;
    jobs[current_index].queue_position = neighbor_position;
    jobs[current_index].save(&tx).await?;
    jobs[neighbor_index].queue_position = current_position;
    jobs[neighbor_index].save(&tx).await?;
    tx.commit().await.map_err(welds::WeldsError::from)?;
    Ok(())
}

pub async fn claim_running(handle: &DataHandle, id: i64) -> Result<bool> {
    let updated = DownloadJob::where_col(|job| job.id.equal(id))
        .where_col(|job| job.status.equal(DownloadJobStatus::Queued.as_str()))
        .set(
            |job| job.status,
            DownloadJobStatus::Running.as_str().to_string(),
        )
        .set(|job| job.updated_at, sqlite_timestamp())
        .run(handle.client())
        .await?;
    Ok(updated == 1)
}

pub async fn is_cancelled(handle: &DataHandle, id: i64) -> Result<bool> {
    Ok(DownloadJob::find_by_id(handle.client(), id)
        .await?
        .is_some_and(|job| job.status == DownloadJobStatus::Cancelled.as_str()))
}

pub async fn is_paused(handle: &DataHandle, id: i64) -> Result<bool> {
    Ok(DownloadJob::find_by_id(handle.client(), id)
        .await?
        .is_some_and(|job| job.status == DownloadJobStatus::Paused.as_str()))
}

pub async fn is_stopped(handle: &DataHandle, id: i64) -> Result<bool> {
    Ok(DownloadJob::find_by_id(handle.client(), id)
        .await?
        .is_some_and(|job| {
            job.status == DownloadJobStatus::Paused.as_str()
                || job.status == DownloadJobStatus::Cancelled.as_str()
        }))
}

pub async fn pause(handle: &DataHandle, id: i64) -> Result<()> {
    let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await? else {
        return Err(DataError::InvalidOperation(
            "only queued or running jobs can be paused".to_string(),
        ));
    };
    if !matches!(
        DownloadJobStatus::parse(&job.status),
        Some(DownloadJobStatus::Queued | DownloadJobStatus::Running)
    ) {
        return Err(DataError::InvalidOperation(
            "only queued or running jobs can be paused".to_string(),
        ));
    }
    job.status = DownloadJobStatus::Paused.as_str().to_string();
    job.download_speed_bps = 0;
    job.updated_at = sqlite_timestamp();
    job.save(handle.client()).await?;
    Ok(())
}

pub async fn resume_paused(handle: &DataHandle, id: i64) -> Result<()> {
    let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await? else {
        return Err(DataError::InvalidOperation(
            "only paused jobs can be resumed".to_string(),
        ));
    };
    if job.status != DownloadJobStatus::Paused.as_str() {
        return Err(DataError::InvalidOperation(
            "only paused jobs can be resumed".to_string(),
        ));
    }
    let job_type = parse_job_type(&job.job_type)?;
    clear_torrent_session(&mut job.payload_json)?;
    job.status = DownloadJobStatus::Queued.as_str().to_string();
    job.error_message = None;
    job.download_speed_bps = 0;
    job.queue_position = next_queue_position(handle, job_type).await?;
    job.updated_at = sqlite_timestamp();
    job.save(handle.client()).await?;
    Ok(())
}

pub async fn update_progress(handle: &DataHandle, id: i64, progress_pct: f64) -> Result<()> {
    update_progress_and_speed(handle, id, progress_pct, None).await
}

pub async fn update_progress_and_speed(
    handle: &DataHandle,
    id: i64,
    progress_pct: f64,
    download_speed_bps: Option<u64>,
) -> Result<()> {
    if let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await? {
        job.progress_pct = progress_pct;
        if let Some(speed) = download_speed_bps {
            job.download_speed_bps = speed as i64;
        }
        job.updated_at = sqlite_timestamp();
        job.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn finish_success(handle: &DataHandle, id: i64) -> Result<()> {
    if let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await?
        && job.status == DownloadJobStatus::Running.as_str()
    {
        job.status = DownloadJobStatus::Completed.as_str().to_string();
        job.progress_pct = 100.0;
        job.updated_at = sqlite_timestamp();
        job.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn finish_failed(handle: &DataHandle, id: i64, error: &str) -> Result<()> {
    if let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await? {
        job.status = DownloadJobStatus::Failed.as_str().to_string();
        job.error_message = Some(error.to_string());
        job.updated_at = sqlite_timestamp();
        job.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn list_completed_torrent_job_ids(handle: &DataHandle) -> Result<Vec<i64>> {
    Ok(DownloadJob::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|job| {
            job.job_type == DownloadJobType::Torrent.as_str()
                && job.status == DownloadJobStatus::Completed.as_str()
        })
        .map(|job| job.id)
        .collect())
}

pub async fn purge_completed(handle: &DataHandle) -> Result<u64> {
    let mut deleted = 0;
    for mut job in DownloadJob::all().run(handle.client()).await? {
        if job.status == DownloadJobStatus::Completed.as_str() {
            job.delete(handle.client()).await?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

pub async fn delete_by_id(handle: &DataHandle, id: i64) -> Result<bool> {
    let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    job.delete(handle.client()).await?;
    Ok(true)
}

pub async fn retry_failed(handle: &DataHandle, id: i64) -> Result<()> {
    let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await? else {
        return Err(DataError::InvalidOperation(format!("job {id} not found")));
    };
    if job.status != DownloadJobStatus::Failed.as_str() {
        return Err(DataError::InvalidOperation(
            "only failed jobs can be retried".to_string(),
        ));
    }
    requeue_failed_job(handle, &mut job).await?;
    Ok(())
}

pub async fn retry_all_failed(handle: &DataHandle) -> Result<u64> {
    let mut jobs = DownloadJob::all().run(handle.client()).await?;
    let mut next_positions = jobs
        .iter()
        .filter(|job| job.status == DownloadJobStatus::Queued.as_str())
        .map(|job| Ok((parse_job_type(&job.job_type)?, job.queue_position)))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .fold(
            HashMap::<DownloadJobType, i64>::new(),
            |mut positions, (job_type, position)| {
                positions
                    .entry(job_type)
                    .and_modify(|max_position| *max_position = (*max_position).max(position))
                    .or_insert(position);
                positions
            },
        );
    jobs.sort_by_key(|job| (job.job_type.clone(), job.queue_position, job.id));

    let mut retried = 0;
    for mut job in jobs {
        if job.status == DownloadJobStatus::Failed.as_str() {
            let job_type = parse_job_type(&job.job_type)?;
            let next_position = next_positions.entry(job_type).or_default();
            *next_position += 1;
            requeue_job_at_position(handle, &mut job, *next_position).await?;
            retried += 1;
        }
    }
    Ok(retried)
}

async fn requeue_failed_job(
    handle: &DataHandle,
    job: &mut welds::state::DbState<DownloadJob>,
) -> Result<()> {
    let job_type = parse_job_type(&job.job_type)?;
    let queue_position = next_queue_position(handle, job_type).await?;
    requeue_job_at_position(handle, job, queue_position).await
}

async fn requeue_job_at_position(
    handle: &DataHandle,
    job: &mut welds::state::DbState<DownloadJob>,
    queue_position: i64,
) -> Result<()> {
    clear_torrent_session(&mut job.payload_json)?;
    job.status = DownloadJobStatus::Queued.as_str().to_string();
    job.error_message = None;
    job.progress_pct = 0.0;
    job.download_speed_bps = 0;
    job.queue_position = queue_position;
    job.updated_at = sqlite_timestamp();
    job.save(handle.client()).await?;
    Ok(())
}

pub async fn cancel(handle: &DataHandle, id: i64) -> Result<bool> {
    let Some(mut job) = DownloadJob::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    if !matches!(
        DownloadJobStatus::parse(&job.status),
        Some(DownloadJobStatus::Queued | DownloadJobStatus::Running | DownloadJobStatus::Paused)
    ) {
        return Ok(false);
    }
    job.status = DownloadJobStatus::Cancelled.as_str().to_string();
    job.updated_at = sqlite_timestamp();
    job.save(handle.client()).await?;
    Ok(true)
}

fn row_from_state(row: welds::state::DbState<DownloadJob>) -> Result<DownloadJobRow> {
    let model = row.into_inner();
    Ok(DownloadJobRow {
        id: model.id,
        status: parse_status(&model.status)?,
        job_type: parse_job_type(&model.job_type)?,
        qobuz_id: model.qobuz_id,
        quality: model.quality,
        progress_pct: model.progress_pct,
        download_speed_bps: model.download_speed_bps,
        queue_position: model.queue_position,
        payload_json: model.payload_json,
        error_message: model.error_message,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

fn parse_status(value: &str) -> Result<DownloadJobStatus> {
    DownloadJobStatus::parse(value)
        .ok_or_else(|| DataError::Config(format!("invalid download job status {value}")))
}

fn parse_job_type(value: &str) -> Result<DownloadJobType> {
    DownloadJobType::parse(value)
        .ok_or_else(|| DataError::Config(format!("invalid download job type {value}")))
}

fn clear_torrent_session(payload_json: &mut Option<String>) -> Result<()> {
    let Some(raw) = payload_json.as_ref() else {
        return Ok(());
    };
    let mut payload: Value = serde_json::from_str(raw)?;
    if let Some(torrent) = payload.get_mut("torrent").and_then(Value::as_object_mut) {
        torrent.remove("librqbit_id");
        torrent.remove("runtime");
    }
    *payload_json = Some(serde_json::to_string(&payload)?);
    Ok(())
}

fn sqlite_timestamp() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::convert_jobs as data;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertTrigger {
    Manual,
    Auto,
}

#[derive(Debug, Clone)]
pub struct ConvertJobRow {
    pub id: i64,
    pub album_id: i64,
    pub status: String,
    pub trigger: String,
    pub files_total: i64,
    pub files_done: i64,
    pub progress_pct: f64,
    pub error_message: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConvertFileStatus {
    pub path: String,
    pub status: String,
    /// Encode progress within this file (0–100), while `status == "running"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn album_has_active_job(pool: &SqlitePool, album_id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::album_has_active_job(&handle, album_id).await?)
}

pub async fn create(
    pool: &SqlitePool,
    album_id: i64,
    trigger: ConvertTrigger,
    files_total: i64,
) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::create(
        &handle,
        album_id,
        convert_trigger_to_data(trigger),
        files_total,
    )
    .await?)
}

pub async fn enqueue_album_if_needed(
    pool: &SqlitePool,
    album_id: i64,
    files_total: i64,
) -> Result<Option<i64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::enqueue_album_if_needed(&handle, album_id, files_total).await?)
}

pub async fn claim_running(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::claim_running(&handle, id).await?)
}

pub async fn next_queued_id(pool: &SqlitePool) -> Result<Option<i64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::next_queued_id(&handle).await?)
}

pub async fn update_progress(
    pool: &SqlitePool,
    id: i64,
    files_done: i64,
    files_total: i64,
    progress_pct: f64,
    payload_json: Option<&str>,
) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::update_progress(
        &handle,
        id,
        files_done,
        files_total,
        progress_pct,
        payload_json,
    )
    .await?)
}

pub async fn finish(
    pool: &SqlitePool,
    id: i64,
    status: ConvertJobStatus,
    error_message: Option<&str>,
    payload_json: Option<&str>,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::finish(
        &handle,
        id,
        convert_status_to_data(status),
        error_message,
        payload_json,
    )
    .await?;
    Ok(())
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<ConvertJobRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::get_by_id(&handle, id)
        .await?
        .map(convert_row_from_data))
}

pub async fn latest_for_album(
    pool: &SqlitePool,
    album_id: i64,
) -> Result<Option<ConvertJobRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::latest_for_album(&handle, album_id)
        .await?
        .map(convert_row_from_data))
}

pub async fn row_to_summary(row: ConvertJobRow) -> Result<crate::api::ConvertJobSummary, ApiError> {
    Ok(crate::api::ConvertJobSummary {
        id: row.id,
        album_id: row.album_id,
        status: row.status,
        trigger: row.trigger,
        files_total: row.files_total,
        files_done: row.files_done,
        progress_pct: row.progress_pct,
        error_message: row.error_message,
        payload_json: row.payload_json,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn convert_status_to_data(status: ConvertJobStatus) -> data::ConvertJobStatus {
    match status {
        ConvertJobStatus::Queued => data::ConvertJobStatus::Queued,
        ConvertJobStatus::Running => data::ConvertJobStatus::Running,
        ConvertJobStatus::Success => data::ConvertJobStatus::Success,
        ConvertJobStatus::Failed => data::ConvertJobStatus::Failed,
        ConvertJobStatus::Cancelled => data::ConvertJobStatus::Cancelled,
    }
}

fn convert_trigger_to_data(trigger: ConvertTrigger) -> data::ConvertTrigger {
    match trigger {
        ConvertTrigger::Manual => data::ConvertTrigger::Manual,
        ConvertTrigger::Auto => data::ConvertTrigger::Auto,
    }
}

fn convert_row_from_data(row: data::ConvertJobRow) -> ConvertJobRow {
    ConvertJobRow {
        id: row.id,
        album_id: row.album_id,
        status: row.status.as_str().to_string(),
        trigger: row.trigger.as_str().to_string(),
        files_total: row.files_total,
        files_done: row.files_done,
        progress_pct: row.progress_pct,
        error_message: row.error_message,
        payload_json: row.payload_json,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::{albums, artists, connect, migrate};

    async fn seed_album(pool: &SqlitePool) -> i64 {
        let artist_id = artists::upsert_by_name(pool, "Artist", None).await.unwrap();
        albums::upsert(
            pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album",
                year: None,
                qobuz_album_id: None,
                path: Some("Artist/Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn progress_updates_do_not_modify_terminal_jobs() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let album_id = seed_album(&pool).await;
        let job_id = create(&pool, album_id, ConvertTrigger::Manual, 2)
            .await
            .unwrap();
        assert!(claim_running(&pool, job_id).await.unwrap());
        finish(&pool, job_id, ConvertJobStatus::Success, None, Some("[]"))
            .await
            .unwrap();

        update_progress(&pool, job_id, 1, 2, 50.0, Some("[{\"path\":\"late\"}]"))
            .await
            .unwrap();

        let row = get_by_id(&pool, job_id).await.unwrap().unwrap();
        assert_eq!(row.status, "success");
        assert_eq!(row.progress_pct, 100.0);
        assert_eq!(row.payload_json.as_deref(), Some("[]"));
    }

    #[tokio::test]
    async fn active_convert_jobs_are_unique_per_album() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let album_id = seed_album(&pool).await;

        create(&pool, album_id, ConvertTrigger::Manual, 1)
            .await
            .unwrap();
        let duplicate = create(&pool, album_id, ConvertTrigger::Auto, 1).await;

        assert!(duplicate.is_err(), "second active job should be rejected");
    }
}

use sqlx::SqlitePool;

use crate::api::LibraryScanRunSummary;
use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::library_scan_runs as data_scan_runs;

impl From<data_scan_runs::LibraryScanRunSummary> for LibraryScanRunSummary {
    fn from(row: data_scan_runs::LibraryScanRunSummary) -> Self {
        Self {
            id: row.id,
            status: row.status,
            files_seen: row.files_seen,
            files_processed: row.files_processed,
            files_indexed: row.files_indexed,
            files_total: row.files_total,
            started_at: row.started_at,
            finished_at: row.finished_at,
            error_message: row.error_message,
        }
    }
}

pub async fn has_running(pool: &SqlitePool) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data_scan_runs::has_running(&handle).await?)
}

pub async fn start(pool: &SqlitePool) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data_scan_runs::start(&handle).await?)
}

pub async fn get_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<LibraryScanRunSummary>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data_scan_runs::get_by_id(&handle, id)
        .await?
        .map(Into::into))
}

pub async fn latest(pool: &SqlitePool) -> Result<Option<LibraryScanRunSummary>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data_scan_runs::latest(&handle).await?.map(Into::into))
}

pub async fn update_progress(
    pool: &SqlitePool,
    id: i64,
    files_seen: i64,
    files_processed: i64,
    files_indexed: i64,
    files_total: i64,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data_scan_runs::update_progress(
        &handle,
        id,
        files_seen,
        files_processed,
        files_indexed,
        files_total,
    )
    .await?;
    Ok(())
}

pub async fn finish_success(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data_scan_runs::finish_success(&handle, id).await?;
    Ok(())
}

pub async fn finish_failed(pool: &SqlitePool, id: i64, error: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data_scan_runs::finish_failed(&handle, id, error).await?;
    Ok(())
}

pub async fn cancel(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data_scan_runs::cancel(&handle, id).await?)
}

pub async fn is_cancelled(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data_scan_runs::is_cancelled(&handle, id).await?)
}

use euterpe_data::DataHandle;
use euterpe_data::repositories::qobuz as data;
use sqlx::SqlitePool;

use crate::api::QobuzSyncRunSummary;
use crate::error::ApiError;

pub async fn latest(pool: &SqlitePool) -> Result<Option<QobuzSyncRunSummary>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::sync_latest(&handle).await?.map(summary_from_data))
}

pub async fn start(pool: &SqlitePool) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::start_sync_run(&handle).await?)
}

pub async fn finish_success(
    pool: &SqlitePool,
    run_id: i64,
    albums_total: i64,
    added: i64,
    removed: i64,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::finish_sync_success(&handle, run_id, albums_total, added, removed).await?)
}

pub async fn finish_failed(pool: &SqlitePool, run_id: i64, error: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::finish_sync_failed(&handle, run_id, error).await?)
}

fn summary_from_data(row: data::QobuzSyncRunSummary) -> QobuzSyncRunSummary {
    QobuzSyncRunSummary {
        id: row.id,
        status: row.status,
        started_at: row.started_at,
        finished_at: row.finished_at,
        albums_total: row.albums_total,
        albums_added: row.albums_added,
        albums_removed: row.albums_removed,
        error_message: row.error_message,
    }
}

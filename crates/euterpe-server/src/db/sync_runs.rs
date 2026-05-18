use sqlx::SqlitePool;

use crate::api::QobuzSyncRunSummary;
use crate::error::ApiError;

#[derive(Debug, sqlx::FromRow)]
struct SyncRunRow {
    id: i64,
    status: String,
    started_at: String,
    finished_at: Option<String>,
    albums_total: Option<i64>,
    albums_added: Option<i64>,
    albums_removed: Option<i64>,
    error_message: Option<String>,
}

impl From<SyncRunRow> for QobuzSyncRunSummary {
    fn from(row: SyncRunRow) -> Self {
        Self {
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
}

pub async fn latest(pool: &SqlitePool) -> Result<Option<QobuzSyncRunSummary>, ApiError> {
    let row: Option<SyncRunRow> = sqlx::query_as(
        r#"
        SELECT id, status, started_at, finished_at, albums_total, albums_added, albums_removed, error_message
        FROM qobuz_sync_runs
        ORDER BY id DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn start(pool: &SqlitePool) -> Result<i64, ApiError> {
    let result = sqlx::query(
        r#"
        INSERT INTO qobuz_sync_runs (started_at, status)
        VALUES (datetime('now'), 'running')
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn finish_success(
    pool: &SqlitePool,
    run_id: i64,
    albums_total: i64,
    added: i64,
    removed: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE qobuz_sync_runs
        SET finished_at = datetime('now'),
            status = 'success',
            albums_total = ?,
            albums_added = ?,
            albums_removed = ?
        WHERE id = ?
        "#,
    )
    .bind(albums_total)
    .bind(added)
    .bind(removed)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_failed(pool: &SqlitePool, run_id: i64, error: &str) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE qobuz_sync_runs
        SET finished_at = datetime('now'),
            status = 'failed',
            error_message = ?
        WHERE id = ?
        "#,
    )
    .bind(error)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

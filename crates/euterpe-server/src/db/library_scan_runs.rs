use sqlx::SqlitePool;

use crate::api::LibraryScanRunSummary;
use crate::error::ApiError;

#[derive(Debug, sqlx::FromRow)]
struct ScanRunRow {
    id: i64,
    status: String,
    files_seen: i64,
    files_indexed: i64,
    started_at: String,
    finished_at: Option<String>,
    error_message: Option<String>,
}

impl From<ScanRunRow> for LibraryScanRunSummary {
    fn from(row: ScanRunRow) -> Self {
        Self {
            id: row.id,
            status: row.status,
            files_seen: row.files_seen,
            files_indexed: row.files_indexed,
            started_at: row.started_at,
            finished_at: row.finished_at,
            error_message: row.error_message,
        }
    }
}

pub async fn has_running(pool: &SqlitePool) -> Result<bool, ApiError> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM library_scan_runs WHERE status = 'running' LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

pub async fn start(pool: &SqlitePool) -> Result<i64, ApiError> {
    let result = sqlx::query(
        r#"
        INSERT INTO library_scan_runs (status, started_at)
        VALUES ('running', datetime('now'))
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<LibraryScanRunSummary>, ApiError> {
    let row: Option<ScanRunRow> = sqlx::query_as(
        r#"
        SELECT id, status, files_seen, files_indexed, started_at, finished_at, error_message
        FROM library_scan_runs WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn latest(pool: &SqlitePool) -> Result<Option<LibraryScanRunSummary>, ApiError> {
    let row: Option<ScanRunRow> = sqlx::query_as(
        r#"
        SELECT id, status, files_seen, files_indexed, started_at, finished_at, error_message
        FROM library_scan_runs
        ORDER BY id DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn update_progress(
    pool: &SqlitePool,
    id: i64,
    files_seen: i64,
    files_indexed: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE library_scan_runs
        SET files_seen = ?, files_indexed = ?
        WHERE id = ?
        "#,
    )
    .bind(files_seen)
    .bind(files_indexed)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_success(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE library_scan_runs
        SET status = 'success', finished_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_failed(pool: &SqlitePool, id: i64, error: &str) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE library_scan_runs
        SET status = 'failed', finished_at = datetime('now'), error_message = ?
        WHERE id = ?
        "#,
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn is_cancelled(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT status FROM library_scan_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(matches!(row, Some((s,)) if s == "cancelled"))
}

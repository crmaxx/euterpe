use sqlx::SqlitePool;

use crate::error::ApiError;

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

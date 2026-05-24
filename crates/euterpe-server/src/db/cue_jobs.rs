use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::ApiError;

#[derive(Debug, Clone, sqlx::FromRow)]
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
    let result = sqlx::query(
        r#"
        INSERT INTO cue_jobs (album_id, status, tracks_total, tracks_done, progress_pct, payload_json)
        VALUES (?, 'queued', ?, 0, 0, ?)
        "#,
    )
    .bind(album_id)
    .bind(tracks_total)
    .bind(payload_json)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn latest_for_album(
    pool: &SqlitePool,
    album_id: i64,
) -> Result<Option<CueJobRow>, ApiError> {
    sqlx::query_as::<_, CueJobRow>(
        r#"
        SELECT * FROM cue_jobs
        WHERE album_id = ?
        ORDER BY id DESC
        LIMIT 1
        "#,
    )
    .bind(album_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn mark_running(pool: &SqlitePool, job_id: i64) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE cue_jobs
        SET status = 'running', updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_success(
    pool: &SqlitePool,
    job_id: i64,
    tracks_done: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE cue_jobs
        SET status = 'success',
            tracks_done = ?,
            progress_pct = 100,
            error_message = NULL,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(tracks_done)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_progress(
    pool: &SqlitePool,
    job_id: i64,
    tracks_done: i64,
    tracks_total: i64,
) -> Result<(), ApiError> {
    let progress_pct = if tracks_total > 0 {
        (tracks_done as f64 / tracks_total as f64) * 100.0
    } else {
        0.0
    };
    sqlx::query(
        r#"
        UPDATE cue_jobs
        SET tracks_done = ?,
            tracks_total = ?,
            progress_pct = ?,
            updated_at = datetime('now')
        WHERE id = ? AND status = 'running'
        "#,
    )
    .bind(tracks_done)
    .bind(tracks_total)
    .bind(progress_pct)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_failed(pool: &SqlitePool, job_id: i64, error: &str) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE cue_jobs
        SET status = 'failed',
            error_message = ?,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(error)
    .bind(job_id)
    .execute(pool)
    .await?;
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

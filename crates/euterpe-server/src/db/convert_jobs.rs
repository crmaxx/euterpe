use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::ApiError;

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

impl ConvertTrigger {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
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
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT id FROM convert_jobs
        WHERE album_id = ? AND status IN ('queued', 'running')
        LIMIT 1
        "#,
    )
    .bind(album_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

pub async fn create(
    pool: &SqlitePool,
    album_id: i64,
    trigger: ConvertTrigger,
    files_total: i64,
) -> Result<i64, ApiError> {
    let result = sqlx::query(
        r#"
        INSERT INTO convert_jobs (album_id, status, trigger, files_total, files_done, progress_pct)
        VALUES (?, 'queued', ?, ?, 0, 0)
        "#,
    )
    .bind(album_id)
    .bind(trigger.as_str())
    .bind(files_total)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn enqueue_album_if_needed(
    pool: &SqlitePool,
    album_id: i64,
    files_total: i64,
) -> Result<Option<i64>, ApiError> {
    if album_has_active_job(pool, album_id).await? {
        return Ok(None);
    }
    let id = create(pool, album_id, ConvertTrigger::Auto, files_total).await?;
    Ok(Some(id))
}

pub async fn claim_running(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE convert_jobs
        SET status = 'running', updated_at = datetime('now')
        WHERE id = ? AND status = 'queued'
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn next_queued_id(pool: &SqlitePool) -> Result<Option<i64>, ApiError> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT id FROM convert_jobs
        WHERE status = 'queued'
        ORDER BY id ASC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

pub async fn update_progress(
    pool: &SqlitePool,
    id: i64,
    files_done: i64,
    files_total: i64,
    progress_pct: f64,
    payload_json: Option<&str>,
) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE convert_jobs
        SET files_done = ?, files_total = ?, progress_pct = ?, payload_json = COALESCE(?, payload_json),
            updated_at = datetime('now')
        WHERE id = ? AND status = 'running'
        "#,
    )
    .bind(files_done)
    .bind(files_total)
    .bind(progress_pct)
    .bind(payload_json)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn finish(
    pool: &SqlitePool,
    id: i64,
    status: ConvertJobStatus,
    error_message: Option<&str>,
    payload_json: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE convert_jobs
        SET status = ?, error_message = ?, payload_json = COALESCE(?, payload_json),
            progress_pct = CASE WHEN ? = 'success' THEN 100.0 ELSE progress_pct END,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(status.as_str())
    .bind(error_message)
    .bind(payload_json)
    .bind(status.as_str())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<ConvertJobRow>, ApiError> {
    sqlx::query_as::<_, ConvertJobRow>("SELECT * FROM convert_jobs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub async fn latest_for_album(
    pool: &SqlitePool,
    album_id: i64,
) -> Result<Option<ConvertJobRow>, ApiError> {
    sqlx::query_as::<_, ConvertJobRow>(
        r#"
        SELECT * FROM convert_jobs
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{albums, artists, connect, migrate};

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

use sqlx::SqlitePool;

use crate::api::{DownloadJob, DownloadJobStatus, DownloadJobType};
use crate::error::ApiError;
use crate::services::download::DownloadJobPayload;

#[derive(Debug, sqlx::FromRow)]
struct JobRow {
    id: i64,
    status: String,
    job_type: String,
    qobuz_id: Option<i64>,
    quality: i32,
    progress_pct: f64,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

impl JobRow {
    fn into_job(self) -> Result<DownloadJob, ApiError> {
        Ok(DownloadJob {
            id: self.id,
            status: DownloadJobStatus::parse(&self.status)
                .ok_or_else(|| ApiError::Config(format!("invalid status {}", self.status)))?,
            job_type: match self.job_type.as_str() {
                "album" => DownloadJobType::Album,
                "track" => DownloadJobType::Track,
                "artist" => DownloadJobType::Artist,
                "playlist" => DownloadJobType::Playlist,
                other => return Err(ApiError::Config(format!("invalid job_type {other}"))),
            },
            qobuz_id: self.qobuz_id.unwrap_or(0),
            quality: self.quality,
            progress_pct: self.progress_pct,
            error_message: self.error_message,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

pub fn can_transition(from: DownloadJobStatus, to: DownloadJobStatus) -> bool {
    use DownloadJobStatus::*;
    matches!(
        (from, to),
        (Queued, Running)
            | (Queued, Cancelled)
            | (Running, Completed)
            | (Running, Failed)
            | (Running, Cancelled)
    )
}

pub async fn insert_queued(
    pool: &SqlitePool,
    job_type: DownloadJobType,
    qobuz_id: u64,
    quality: u8,
    payload: Option<&DownloadJobPayload>,
) -> Result<i64, ApiError> {
    let payload_json = payload
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| ApiError::Message(format!("job payload: {e}")))?;
    let result = sqlx::query(
        r#"
        INSERT INTO download_jobs (status, job_type, qobuz_id, quality, payload_json)
        VALUES ('queued', ?, ?, ?, ?)
        "#,
    )
    .bind(job_type.as_str())
    .bind(qobuz_id as i64)
    .bind(quality as i32)
    .bind(payload_json)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get_payload(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<DownloadJobPayload>, ApiError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT payload_json FROM download_jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let Some((json,)) = row else {
        return Ok(None);
    };
    let Some(json) = json else {
        return Ok(Some(DownloadJobPayload::default()));
    };
    let payload: DownloadJobPayload = serde_json::from_str(&json)
        .map_err(|e| ApiError::Message(format!("invalid job payload: {e}")))?;
    Ok(Some(payload))
}

pub async fn has_running_album(
    pool: &SqlitePool,
    album_api_id: &str,
    qobuz_id: Option<u64>,
    quality: u8,
) -> Result<bool, ApiError> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM download_jobs
        WHERE status = 'running' AND job_type = 'album' AND quality = ?
          AND (
            json_extract(payload_json, '$.album_api_id') = ?
            OR (? IS NOT NULL AND qobuz_id = ?)
          )
        "#,
    )
    .bind(quality as i32)
    .bind(album_api_id)
    .bind(qobuz_id.map(|id| id as i64))
    .bind(qobuz_id.map(|id| id as i64))
    .fetch_one(pool)
    .await?;
    Ok(row.0 > 0)
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<DownloadJob>, ApiError> {
    let row: Option<JobRow> = sqlx::query_as("SELECT * FROM download_jobs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.map(|r| r.into_job()).transpose()
}

pub async fn list(pool: &SqlitePool, status: Option<DownloadJobStatus>) -> Result<Vec<DownloadJob>, ApiError> {
    let rows: Vec<JobRow> = if let Some(s) = status {
        sqlx::query_as("SELECT * FROM download_jobs WHERE status = ? ORDER BY id DESC")
            .bind(s.as_str())
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as("SELECT * FROM download_jobs ORDER BY id DESC")
            .fetch_all(pool)
            .await?
    };
    rows.into_iter().map(|r| r.into_job()).collect()
}

pub async fn claim_running(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE download_jobs
        SET status = 'running', updated_at = datetime('now')
        WHERE id = ? AND status = 'queued'
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn is_cancelled(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT status FROM download_jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(s,)| s == "cancelled").unwrap_or(false))
}

pub async fn update_progress(pool: &SqlitePool, id: i64, progress_pct: f64) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE download_jobs
        SET progress_pct = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(progress_pct)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_success(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE download_jobs
        SET status = 'completed', progress_pct = 100, updated_at = datetime('now')
        WHERE id = ? AND status = 'running'
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
        UPDATE download_jobs
        SET status = 'failed', error_message = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub fn is_terminal_status(status: DownloadJobStatus) -> bool {
    matches!(
        status,
        DownloadJobStatus::Completed | DownloadJobStatus::Failed | DownloadJobStatus::Cancelled
    )
}

/// Remove all jobs that are not `queued` or `running`.
pub async fn purge_finished(pool: &SqlitePool) -> Result<u64, ApiError> {
    let result = sqlx::query(
        r#"
        DELETE FROM download_jobs
        WHERE status IN ('completed', 'failed', 'cancelled')
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Permanently delete a job row. Caller must enforce terminal-only for active jobs.
pub async fn delete_by_id(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let result = sqlx::query("DELETE FROM download_jobs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn cancel(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE download_jobs
        SET status = 'cancelled', updated_at = datetime('now')
        WHERE id = ? AND status IN ('queued', 'running')
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_legal_transitions() {
        assert!(can_transition(DownloadJobStatus::Queued, DownloadJobStatus::Running));
        assert!(can_transition(
            DownloadJobStatus::Running,
            DownloadJobStatus::Completed
        ));
        assert!(!can_transition(
            DownloadJobStatus::Completed,
            DownloadJobStatus::Running
        ));
    }

    #[tokio::test]
    async fn claim_running_only_from_queued() {
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();
        let id = insert_queued(&pool, DownloadJobType::Album, 42, 6, None)
            .await
            .unwrap();
        assert!(claim_running(&pool, id).await.unwrap());
        assert!(!claim_running(&pool, id).await.unwrap());
    }

    #[tokio::test]
    async fn purge_finished_removes_terminal_jobs_only() {
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();

        let queued = insert_queued(&pool, DownloadJobType::Album, 1, 6, None)
            .await
            .unwrap();
        let running = insert_queued(&pool, DownloadJobType::Album, 2, 6, None)
            .await
            .unwrap();
        claim_running(&pool, running).await.unwrap();
        let done = insert_queued(&pool, DownloadJobType::Album, 3, 6, None)
            .await
            .unwrap();
        claim_running(&pool, done).await.unwrap();
        finish_success(&pool, done).await.unwrap();
        let failed = insert_queued(&pool, DownloadJobType::Album, 4, 6, None)
            .await
            .unwrap();
        finish_failed(&pool, failed, "err").await.unwrap();

        let n = purge_finished(&pool).await.unwrap();
        assert_eq!(n, 2);

        assert!(get(&pool, queued).await.unwrap().is_some());
        assert!(get(&pool, running).await.unwrap().is_some());
        assert!(get(&pool, done).await.unwrap().is_none());
        assert!(get(&pool, failed).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_by_id_removes_row() {
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();
        let id = insert_queued(&pool, DownloadJobType::Album, 1, 6, None)
            .await
            .unwrap();
        claim_running(&pool, id).await.unwrap();
        finish_success(&pool, id).await.unwrap();
        assert!(delete_by_id(&pool, id).await.unwrap());
        assert!(get(&pool, id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn has_running_album_matches_payload_album_api_id() {
        use crate::services::download::DownloadJobPayload;

        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();
        let payload = DownloadJobPayload {
            album_api_id: Some("zg7pv28g4mldg".into()),
        };
        let id = insert_queued(
            &pool,
            DownloadJobType::Album,
            0,
            6,
            Some(&payload),
        )
        .await
        .unwrap();
        claim_running(&pool, id).await.unwrap();

        assert!(
            has_running_album(&pool, "zg7pv28g4mldg", None, 6)
                .await
                .unwrap()
        );
        assert!(
            !has_running_album(&pool, "other-album", None, 6)
                .await
                .unwrap()
        );
    }
}

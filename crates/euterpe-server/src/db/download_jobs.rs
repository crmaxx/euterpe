use serde_json::json;
use sqlx::SqlitePool;

use crate::api::keyset::{
    decode_cursor, ensure_cursor_matches, finish_keyset_page, fingerprint_json, keyset_and_clause,
};
use crate::api::{
    DownloadJob, DownloadJobStatus, DownloadJobType, KeysetPage, SortKeyKind,
    SortKeyValue, SortOrder,
};
use crate::error::ApiError;

fn bind_sort_keys<'q, T>(
    mut query: sqlx::query::QueryAs<'q, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments<'q>>,
    binds: &'q [SortKeyValue],
) -> sqlx::query::QueryAs<'q, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments<'q>> {
    for b in binds {
        query = match b {
            SortKeyValue::Text(s) => query.bind(s),
            SortKeyValue::Int(n) => query.bind(n),
            SortKeyValue::Bool(n) => query.bind(n),
        };
    }
    query
}
use crate::services::download::DownloadJobPayload;

#[derive(Debug, sqlx::FromRow)]
struct JobRow {
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

impl JobRow {
    fn into_job(self) -> Result<DownloadJob, ApiError> {
        let job_type = match self.job_type.as_str() {
            "album" => DownloadJobType::Album,
            "track" => DownloadJobType::Track,
            "artist" => DownloadJobType::Artist,
            "playlist" => DownloadJobType::Playlist,
            "torrent" => DownloadJobType::Torrent,
            other => return Err(ApiError::Config(format!("invalid job_type {other}"))),
        };
        let payload: DownloadJobPayload = self
            .payload_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| ApiError::Message(format!("invalid job payload: {e}")))?
            .unwrap_or_default();
        Ok(DownloadJob {
            id: self.id,
            status: DownloadJobStatus::parse(&self.status)
                .ok_or_else(|| ApiError::Config(format!("invalid status {}", self.status)))?,
            job_type,
            source: payload.source(job_type),
            display_title: payload.display_title(job_type),
            qobuz_id: self.qobuz_id.unwrap_or(0),
            quality: self.quality,
            progress_pct: self.progress_pct,
            download_speed_bps: self.download_speed_bps.max(0) as u64,
            queue_position: self.queue_position,
            torrent_detail: payload.torrent_detail_for_api(),
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

pub async fn next_queue_position(
    pool: &SqlitePool,
    job_type: DownloadJobType,
) -> Result<i64, ApiError> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(MAX(queue_position), 0) + 1
        FROM download_jobs
        WHERE job_type = ? AND status = 'queued'
        "#,
    )
    .bind(job_type.as_str())
    .fetch_one(pool)
    .await?;
    Ok(row.0)
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
    let queue_position = next_queue_position(pool, job_type).await?;
    let result = sqlx::query(
        r#"
        INSERT INTO download_jobs (status, job_type, qobuz_id, quality, queue_position, payload_json)
        VALUES ('queued', ?, ?, ?, ?, ?)
        "#,
    )
    .bind(job_type.as_str())
    .bind(qobuz_id as i64)
    .bind(quality as i32)
    .bind(queue_position)
    .bind(payload_json)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn set_payload(
    pool: &SqlitePool,
    id: i64,
    payload: &DownloadJobPayload,
) -> Result<(), ApiError> {
    let payload_json = serde_json::to_string(payload)
        .map_err(|e| ApiError::Message(format!("job payload: {e}")))?;
    sqlx::query(
        r#"
        UPDATE download_jobs
        SET payload_json = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(payload_json)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadsSort {
    Id,
    CreatedAt,
    Status,
    QueuePosition,
}

impl DownloadsSort {
    pub fn parse(s: &str) -> Result<Self, ApiError> {
        match s {
            "id" => Ok(Self::Id),
            "created_at" => Ok(Self::CreatedAt),
            "status" => Ok(Self::Status),
            "queue_position" => Ok(Self::QueuePosition),
            _ => Err(ApiError::bad_request(
                "sort must be id, created_at, status, or queue_position",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::CreatedAt => "created_at",
            Self::Status => "status",
            Self::QueuePosition => "queue_position",
        }
    }

    fn sort_sql(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::CreatedAt => "created_at",
            Self::Status => "status",
            Self::QueuePosition => "queue_position",
        }
    }

    fn key_kind(self) -> SortKeyKind {
        match self {
            Self::Id | Self::QueuePosition => SortKeyKind::Int,
            _ => SortKeyKind::Text,
        }
    }

    fn order_sql(self, order: SortOrder) -> String {
        let dir = match order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        };
        format!("{} {dir}, id ASC", self.sort_sql())
    }

    fn primary_key(self, row: &JobRow) -> SortKeyValue {
        match self {
            Self::Id => SortKeyValue::Int(row.id),
            Self::CreatedAt => SortKeyValue::Text(row.created_at.clone()),
            Self::Status => SortKeyValue::Text(row.status.clone()),
            Self::QueuePosition => SortKeyValue::Int(row.queue_position),
        }
    }
}

pub async fn count_running_by_type(
    pool: &SqlitePool,
    job_type: DownloadJobType,
) -> Result<u64, ApiError> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM download_jobs
        WHERE status = 'running' AND job_type = ?
        "#,
    )
    .bind(job_type.as_str())
    .fetch_one(pool)
    .await?;
    Ok(row.0.max(0) as u64)
}

/// Next queued job id for the scheduler (highest priority = lowest queue_position).
pub async fn next_queued_id(
    pool: &SqlitePool,
    job_type: DownloadJobType,
) -> Result<Option<i64>, ApiError> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT id FROM download_jobs
        WHERE status = 'queued' AND job_type = ?
        ORDER BY queue_position ASC, id ASC
        LIMIT 1
        "#,
    )
    .bind(job_type.as_str())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityDirection {
    Up,
    Down,
}

pub async fn adjust_queue_priority(
    pool: &SqlitePool,
    id: i64,
    direction: PriorityDirection,
) -> Result<(), ApiError> {
    let row: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT status, job_type, queue_position FROM download_jobs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some((status, job_type, pos)) = row else {
        return Err(ApiError::Message(format!("job {id} not found")));
    };

    if status != "queued" {
        return Err(ApiError::bad_request("only queued jobs can be reordered"));
    }

    let neighbor: Option<(i64, i64)> = match direction {
        PriorityDirection::Up => sqlx::query_as(
            r#"
            SELECT id, queue_position FROM download_jobs
            WHERE status = 'queued' AND job_type = ?
              AND (queue_position < ? OR (queue_position = ? AND id < ?))
            ORDER BY queue_position DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(&job_type)
        .bind(pos)
        .bind(pos)
        .bind(id)
        .fetch_optional(pool)
        .await?,
        PriorityDirection::Down => sqlx::query_as(
            r#"
            SELECT id, queue_position FROM download_jobs
            WHERE status = 'queued' AND job_type = ?
              AND (queue_position > ? OR (queue_position = ? AND id > ?))
            ORDER BY queue_position ASC, id ASC
            LIMIT 1
            "#,
        )
        .bind(&job_type)
        .bind(pos)
        .bind(pos)
        .bind(id)
        .fetch_optional(pool)
        .await?,
    };

    let Some((neighbor_id, neighbor_pos)) = neighbor else {
        return Ok(());
    };

    let mut tx = pool.begin().await.map_err(|e| ApiError::Message(e.to_string()))?;
    sqlx::query("UPDATE download_jobs SET queue_position = ? WHERE id = ?")
        .bind(neighbor_pos)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    sqlx::query("UPDATE download_jobs SET queue_position = ? WHERE id = ?")
        .bind(pos)
        .bind(neighbor_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    tx.commit().await.map_err(|e| ApiError::Message(e.to_string()))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct DownloadsListParams {
    pub sort: DownloadsSort,
    pub order: SortOrder,
    pub limit: u32,
    pub status: Option<DownloadJobStatus>,
    pub cursor: Option<String>,
}

pub async fn list_keyset(
    pool: &SqlitePool,
    params: DownloadsListParams,
) -> Result<KeysetPage<DownloadJob>, ApiError> {
    let fingerprint = fingerprint_json(&json!({
        "status": params.status.map(|s| s.as_str()),
    }));

    let mut keyset_clause = String::new();
    let mut keyset_binds: Vec<SortKeyValue> = Vec::new();
    if let Some(ref cursor_str) = params.cursor {
        let payload = decode_cursor(cursor_str)?;
        let (primary, tie) = ensure_cursor_matches(
            &payload,
            params.sort.as_str(),
            params.order,
            &fingerprint,
            params.sort.key_kind(),
        )?;
        let (clause, binds) = keyset_and_clause(
            params.order,
            params.sort.sort_sql(),
            "id",
            &primary,
            tie,
        );
        keyset_clause = clause;
        keyset_binds = binds;
    }

    let mut status_clause = String::new();
    let status_bind: Option<String> = params.status.map(|s| s.as_str().to_string());

    if status_bind.is_some() {
        status_clause = " AND status = ?".to_string();
    }

    let fetch_limit = (params.limit as i64) + 1;
    let order_by = params.sort.order_sql(params.order);
    let sql = format!(
        "SELECT * FROM download_jobs WHERE 1=1{status_clause} {keyset_clause} ORDER BY {order_by} LIMIT ?"
    );

    let mut query = sqlx::query_as::<_, JobRow>(&sql);
    if let Some(ref st) = status_bind {
        query = query.bind(st);
    }
    query = bind_sort_keys(query, &keyset_binds);
    query = query.bind(fetch_limit);

    let rows: Vec<JobRow> = query.fetch_all(pool).await?;
    let sort = params.sort;
    let page = finish_keyset_page(
        rows,
        params.limit as usize,
        sort.as_str(),
        params.order,
        &fingerprint,
        |r| (sort.primary_key(r), r.id),
    );

    let mut items = Vec::with_capacity(page.items.len());
    for row in page.items {
        items.push(row.into_job()?);
    }

    Ok(KeysetPage {
        items,
        next_cursor: page.next_cursor,
        has_more: page.has_more,
    })
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
    update_progress_and_speed(pool, id, progress_pct, None).await
}

pub async fn update_progress_and_speed(
    pool: &SqlitePool,
    id: i64,
    progress_pct: f64,
    download_speed_bps: Option<u64>,
) -> Result<(), ApiError> {
    if let Some(speed) = download_speed_bps {
        sqlx::query(
            r#"
            UPDATE download_jobs
            SET progress_pct = ?, download_speed_bps = ?, updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(progress_pct)
        .bind(speed as i64)
        .bind(id)
        .execute(pool)
        .await?;
    } else {
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
    }
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
    async fn adjust_queue_priority_swaps_neighbors() {
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();

        let a = insert_queued(&pool, DownloadJobType::Album, 1, 6, None)
            .await
            .unwrap();
        let b = insert_queued(&pool, DownloadJobType::Album, 2, 6, None)
            .await
            .unwrap();
        let c = insert_queued(&pool, DownloadJobType::Torrent, 0, 0, None)
            .await
            .unwrap();

        let pos_a = sqlx::query_as::<_, (i64,)>(
            "SELECT queue_position FROM download_jobs WHERE id = ?",
        )
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap()
        .0;
        let pos_b = sqlx::query_as::<_, (i64,)>(
            "SELECT queue_position FROM download_jobs WHERE id = ?",
        )
        .bind(b)
        .fetch_one(&pool)
        .await
        .unwrap()
        .0;
        assert!(pos_a < pos_b);

        adjust_queue_priority(&pool, b, PriorityDirection::Up)
            .await
            .unwrap();
        let pos_a2 = sqlx::query_as::<_, (i64,)>(
            "SELECT queue_position FROM download_jobs WHERE id = ?",
        )
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap()
        .0;
        let pos_b2 = sqlx::query_as::<_, (i64,)>(
            "SELECT queue_position FROM download_jobs WHERE id = ?",
        )
        .bind(b)
        .fetch_one(&pool)
        .await
        .unwrap()
        .0;
        assert_eq!(pos_a, pos_b2);
        assert_eq!(pos_b, pos_a2);

        let next_album = next_queued_id(&pool, DownloadJobType::Album)
            .await
            .unwrap();
        assert_eq!(next_album, Some(b));

        let next_torrent = next_queued_id(&pool, DownloadJobType::Torrent)
            .await
            .unwrap();
        assert_eq!(next_torrent, Some(c));
    }

    #[tokio::test]
    async fn has_running_album_matches_payload_album_api_id() {
        use crate::services::download::DownloadJobPayload;

        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();
        let payload = DownloadJobPayload {
            album_api_id: Some("zg7pv28g4mldg".into()),
            display_title: None,
            torrent: None,
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

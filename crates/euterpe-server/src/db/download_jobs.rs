use serde_json::json;
use sqlx::SqlitePool;
use std::cmp::Ordering;

use crate::api::keyset::{
    decode_cursor, ensure_cursor_matches, fingerprint_json, finish_keyset_page,
};
use crate::api::{
    DownloadJob, DownloadJobStatus, DownloadJobType, KeysetPage, SortKeyKind, SortKeyValue,
    SortOrder,
};
use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::download_jobs as data;

use crate::services::download::DownloadJobPayload;

/// Tolerate corrupt or legacy `payload_json` so one bad row does not break `GET /downloads`.
fn parse_job_payload(job_id: i64, raw: Option<&str>) -> DownloadJobPayload {
    let Some(raw) = raw else {
        return DownloadJobPayload::default();
    };
    match serde_json::from_str(raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(
                job_id,
                error = %e,
                "download job payload JSON invalid; listing job with empty payload"
            );
            DownloadJobPayload::default()
        }
    }
}

#[derive(Debug)]
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
        let payload = parse_job_payload(self.id, self.payload_json.as_deref());
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
    data::can_transition(download_status_to_data(from), download_status_to_data(to))
}

pub async fn next_queue_position(
    pool: &SqlitePool,
    job_type: DownloadJobType,
) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::next_queue_position(&handle, download_type_to_data(job_type)).await?)
}

pub async fn insert_queued(
    pool: &SqlitePool,
    job_type: DownloadJobType,
    qobuz_id: u64,
    quality: u8,
    payload: Option<&DownloadJobPayload>,
) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::insert_queued(
        &handle,
        download_type_to_data(job_type),
        Some(qobuz_id as i64),
        quality,
        payload,
    )
    .await?)
}

pub async fn set_payload(
    pool: &SqlitePool,
    id: i64,
    payload: &DownloadJobPayload,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::set_payload(&handle, id, payload).await?;
    Ok(())
}

pub async fn get_payload(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<DownloadJobPayload>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::get_payload(&handle, id).await?)
}

pub async fn has_running_album(
    pool: &SqlitePool,
    album_api_id: &str,
    qobuz_id: Option<u64>,
    quality: u8,
) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::has_running_album(&handle, album_api_id, qobuz_id, quality).await?)
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<DownloadJob>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    let row = data::get_by_id(&handle, id).await?.map(job_row_from_data);
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

    fn key_kind(self) -> SortKeyKind {
        match self {
            Self::Id | Self::QueuePosition => SortKeyKind::Int,
            _ => SortKeyKind::Text,
        }
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
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::count_running_by_type(&handle, download_type_to_data(job_type)).await?)
}

/// Next queued job id for the scheduler (highest priority = lowest queue_position).
pub async fn next_queued_id(
    pool: &SqlitePool,
    job_type: DownloadJobType,
) -> Result<Option<i64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::next_queued_id(&handle, download_type_to_data(job_type)).await?)
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
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::adjust_queue_priority(&handle, id, priority_direction_to_data(direction)).await?;
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

    let mut after: Option<(SortKeyValue, i64)> = None;
    if let Some(ref cursor_str) = params.cursor {
        let payload = decode_cursor(cursor_str)?;
        let (primary, tie) = ensure_cursor_matches(
            &payload,
            params.sort.as_str(),
            params.order,
            &fingerprint,
            params.sort.key_kind(),
        )?;
        after = Some((primary, tie));
    }

    let handle = DataHandle::from_sqlite_pool(pool.clone());
    let mut rows: Vec<JobRow> = data::list(&handle)
        .await?
        .into_iter()
        .map(job_row_from_data)
        .filter(|row| {
            params
                .status
                .is_none_or(|status| row.status == status.as_str())
        })
        .collect();
    rows.sort_by(|left, right| compare_job_rows(left, right, params.sort, params.order));
    if let Some((primary, tie)) = after.as_ref() {
        rows.retain(|row| row_is_after_cursor(row, params.sort, params.order, primary, *tie));
    }
    rows.truncate(params.limit as usize + 1);
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
        let job_id = row.id;
        match row.into_job() {
            Ok(job) => items.push(job),
            Err(e) => {
                tracing::error!(
                    job_id,
                    error = %e,
                    "skipping download job row in list"
                );
            }
        }
    }

    Ok(KeysetPage {
        items,
        next_cursor: page.next_cursor,
        has_more: page.has_more,
    })
}

pub async fn claim_running(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::claim_running(&handle, id).await?)
}

pub async fn is_cancelled(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::is_cancelled(&handle, id).await?)
}

pub async fn is_paused(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::is_paused(&handle, id).await?)
}

/// Worker should stop without marking failed/completed (cancelled or paused).
pub async fn is_stopped(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::is_stopped(&handle, id).await?)
}

/// Pause a queued or running job; frees scheduler slots for the next queued job.
pub async fn pause(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::pause(&handle, id).await?;
    Ok(())
}

/// Resume a paused job at the end of its type queue.
pub async fn resume_paused(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::resume_paused(&handle, id).await?;
    Ok(())
}

pub async fn update_progress(
    pool: &SqlitePool,
    id: i64,
    progress_pct: f64,
) -> Result<(), ApiError> {
    update_progress_and_speed(pool, id, progress_pct, None).await
}

pub async fn update_progress_and_speed(
    pool: &SqlitePool,
    id: i64,
    progress_pct: f64,
    download_speed_bps: Option<u64>,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::update_progress_and_speed(&handle, id, progress_pct, download_speed_bps).await?;
    Ok(())
}

pub async fn finish_success(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::finish_success(&handle, id).await?;
    Ok(())
}

pub async fn finish_failed(pool: &SqlitePool, id: i64, error: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::finish_failed(&handle, id, error).await?;
    Ok(())
}

pub fn is_terminal_status(status: DownloadJobStatus) -> bool {
    data::is_terminal_status(download_status_to_data(status))
}

/// Terminal torrent jobs (for incoming dir cleanup before purge).
pub async fn list_terminal_torrent_job_ids(pool: &SqlitePool) -> Result<Vec<i64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::list_terminal_torrent_job_ids(&handle).await?)
}

/// Remove all jobs that are not `queued` or `running`.
pub async fn purge_finished(pool: &SqlitePool) -> Result<u64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::purge_finished(&handle).await?)
}

/// Permanently delete a job row. Caller must enforce terminal-only for active jobs.
pub async fn delete_by_id(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::delete_by_id(&handle, id).await?)
}

/// Re-queue a failed job at the end of its type group (FIFO).
pub async fn retry_failed(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data::retry_failed(&handle, id).await?;
    Ok(())
}

pub async fn cancel(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::cancel(&handle, id).await?)
}

fn job_row_from_data(row: data::DownloadJobRow) -> JobRow {
    JobRow {
        id: row.id,
        status: row.status.as_str().to_string(),
        job_type: row.job_type.as_str().to_string(),
        qobuz_id: row.qobuz_id,
        quality: row.quality,
        progress_pct: row.progress_pct,
        download_speed_bps: row.download_speed_bps,
        queue_position: row.queue_position,
        payload_json: row.payload_json,
        error_message: row.error_message,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn download_status_to_data(status: DownloadJobStatus) -> data::DownloadJobStatus {
    match status {
        DownloadJobStatus::Queued => data::DownloadJobStatus::Queued,
        DownloadJobStatus::Running => data::DownloadJobStatus::Running,
        DownloadJobStatus::Paused => data::DownloadJobStatus::Paused,
        DownloadJobStatus::Completed => data::DownloadJobStatus::Completed,
        DownloadJobStatus::Failed => data::DownloadJobStatus::Failed,
        DownloadJobStatus::Cancelled => data::DownloadJobStatus::Cancelled,
    }
}

fn download_type_to_data(job_type: DownloadJobType) -> data::DownloadJobType {
    match job_type {
        DownloadJobType::Album => data::DownloadJobType::Album,
        DownloadJobType::Track => data::DownloadJobType::Track,
        DownloadJobType::Artist => data::DownloadJobType::Artist,
        DownloadJobType::Playlist => data::DownloadJobType::Playlist,
        DownloadJobType::Torrent => data::DownloadJobType::Torrent,
    }
}

fn priority_direction_to_data(direction: PriorityDirection) -> data::PriorityDirection {
    match direction {
        PriorityDirection::Up => data::PriorityDirection::Up,
        PriorityDirection::Down => data::PriorityDirection::Down,
    }
}

fn compare_job_rows(
    left: &JobRow,
    right: &JobRow,
    sort: DownloadsSort,
    order: SortOrder,
) -> Ordering {
    let primary = compare_sort_values(&sort.primary_key(left), &sort.primary_key(right));
    let ordered = match order {
        SortOrder::Asc => primary,
        SortOrder::Desc => primary.reverse(),
    };
    ordered.then_with(|| left.id.cmp(&right.id))
}

fn row_is_after_cursor(
    row: &JobRow,
    sort: DownloadsSort,
    order: SortOrder,
    primary: &SortKeyValue,
    tie_id: i64,
) -> bool {
    let row_primary = sort.primary_key(row);
    match compare_sort_values(&row_primary, primary) {
        Ordering::Greater => order == SortOrder::Asc,
        Ordering::Less => order == SortOrder::Desc,
        Ordering::Equal => row.id > tie_id,
    }
}

fn compare_sort_values(left: &SortKeyValue, right: &SortKeyValue) -> Ordering {
    match (left, right) {
        (SortKeyValue::Text(left), SortKeyValue::Text(right)) => left.cmp(right),
        (SortKeyValue::Int(left), SortKeyValue::Int(right)) => left.cmp(right),
        (SortKeyValue::Bool(left), SortKeyValue::Bool(right)) => left.cmp(right),
        _ => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_legal_transitions() {
        assert!(can_transition(
            DownloadJobStatus::Queued,
            DownloadJobStatus::Running
        ));
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
    async fn pause_running_allows_next_queued() {
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();

        let running = insert_queued(&pool, DownloadJobType::Album, 1, 6, None)
            .await
            .unwrap();
        claim_running(&pool, running).await.unwrap();

        let queued = insert_queued(&pool, DownloadJobType::Album, 2, 6, None)
            .await
            .unwrap();

        pause(&pool, running).await.unwrap();

        let job = get(&pool, running).await.unwrap().expect("job");
        assert_eq!(job.status, DownloadJobStatus::Paused);

        assert_eq!(
            next_queued_id(&pool, DownloadJobType::Album).await.unwrap(),
            Some(queued)
        );
    }

    #[tokio::test]
    async fn retry_failed_requeues_at_end() {
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();

        let id = insert_queued(&pool, DownloadJobType::Album, 1, 6, None)
            .await
            .unwrap();
        claim_running(&pool, id).await.unwrap();
        finish_failed(&pool, id, "network").await.unwrap();

        let other = insert_queued(&pool, DownloadJobType::Album, 2, 6, None)
            .await
            .unwrap();

        retry_failed(&pool, id).await.unwrap();

        let job = get(&pool, id).await.unwrap().expect("job");
        assert_eq!(job.status, DownloadJobStatus::Queued);
        assert_eq!(job.progress_pct, 0.0);
        assert!(job.error_message.is_none());
        assert!(job.queue_position > 0);

        assert_eq!(
            next_queued_id(&pool, DownloadJobType::Album).await.unwrap(),
            Some(other)
        );
        claim_running(&pool, other).await.unwrap();
        assert_eq!(
            next_queued_id(&pool, DownloadJobType::Album).await.unwrap(),
            Some(id)
        );
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

        let pos_a = get(&pool, a).await.unwrap().unwrap().queue_position;
        let pos_b = get(&pool, b).await.unwrap().unwrap().queue_position;
        assert!(pos_a < pos_b);

        adjust_queue_priority(&pool, b, PriorityDirection::Up)
            .await
            .unwrap();
        let pos_a2 = get(&pool, a).await.unwrap().unwrap().queue_position;
        let pos_b2 = get(&pool, b).await.unwrap().unwrap().queue_position;
        assert_eq!(pos_a, pos_b2);
        assert_eq!(pos_b, pos_a2);

        let next_album = next_queued_id(&pool, DownloadJobType::Album).await.unwrap();
        assert_eq!(next_album, Some(b));

        let next_torrent = next_queued_id(&pool, DownloadJobType::Torrent)
            .await
            .unwrap();
        assert_eq!(next_torrent, Some(c));

        let mut album_positions = Vec::new();
        for id in [a, b] {
            album_positions.push(get(&pool, id).await.unwrap().unwrap().queue_position);
        }
        album_positions.sort_unstable();
        album_positions.dedup();
        assert_eq!(album_positions.len(), 2);
    }

    #[tokio::test]
    async fn invalid_lifecycle_operations_are_bad_requests() {
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();

        let queued = insert_queued(&pool, DownloadJobType::Album, 1, 6, None)
            .await
            .unwrap();

        assert_eq!(
            resume_paused(&pool, queued).await.unwrap_err().status(),
            axum::http::StatusCode::BAD_REQUEST
        );
        assert_eq!(
            retry_failed(&pool, queued).await.unwrap_err().status(),
            axum::http::StatusCode::BAD_REQUEST
        );

        claim_running(&pool, queued).await.unwrap();
        assert_eq!(
            adjust_queue_priority(&pool, queued, PriorityDirection::Up)
                .await
                .unwrap_err()
                .status(),
            axum::http::StatusCode::BAD_REQUEST
        );

        finish_success(&pool, queued).await.unwrap();
        assert_eq!(
            pause(&pool, queued).await.unwrap_err().status(),
            axum::http::StatusCode::BAD_REQUEST
        );
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
        let id = insert_queued(&pool, DownloadJobType::Album, 0, 6, Some(&payload))
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

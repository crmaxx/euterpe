use crate::connection::DataHandle;
use crate::error::Result;
use welds::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryScanRunSummary {
    pub id: i64,
    pub status: String,
    pub files_seen: i64,
    pub files_processed: i64,
    pub files_indexed: i64,
    pub files_total: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "library_scan_runs")]
struct LibraryScanRun {
    #[welds(primary_key)]
    id: i64,
    status: String,
    files_seen: i64,
    files_processed: i64,
    files_indexed: i64,
    files_total: i64,
    started_at: String,
    finished_at: Option<String>,
    error_message: Option<String>,
}

pub async fn has_running(handle: &DataHandle) -> Result<bool> {
    Ok(LibraryScanRun::all()
        .run(handle.client())
        .await?
        .iter()
        .any(|run| run.status == "running"))
}

pub async fn start(handle: &DataHandle) -> Result<i64> {
    let mut run = LibraryScanRun::new();
    run.status = "running".to_string();
    run.files_seen = 0;
    run.files_processed = 0;
    run.files_indexed = 0;
    run.files_total = 0;
    run.started_at = sqlite_timestamp();
    run.finished_at = None;
    run.error_message = None;
    run.save(handle.client()).await?;
    Ok(run.id)
}

pub async fn get_by_id(handle: &DataHandle, id: i64) -> Result<Option<LibraryScanRunSummary>> {
    Ok(LibraryScanRun::find_by_id(handle.client(), id)
        .await?
        .map(summary_from_model))
}

pub async fn latest(handle: &DataHandle) -> Result<Option<LibraryScanRunSummary>> {
    Ok(LibraryScanRun::all()
        .run(handle.client())
        .await?
        .into_iter()
        .max_by_key(|run| run.id)
        .map(summary_from_model))
}

pub async fn update_progress(
    handle: &DataHandle,
    id: i64,
    files_seen: i64,
    files_processed: i64,
    files_indexed: i64,
    files_total: i64,
) -> Result<()> {
    LibraryScanRun::where_col(|run| run.id.equal(id))
        .where_col(|run| run.status.equal("running"))
        .set(|run| run.files_seen, files_seen)
        .set(|run| run.files_processed, files_processed)
        .set(|run| run.files_indexed, files_indexed)
        .set(|run| run.files_total, files_total)
        .run(handle.client())
        .await?;
    Ok(())
}

pub async fn finish_success(handle: &DataHandle, id: i64) -> Result<()> {
    LibraryScanRun::where_col(|run| run.id.equal(id))
        .where_col(|run| run.status.equal("running"))
        .set(|run| run.status, "success".to_string())
        .set(|run| run.finished_at, Some(sqlite_timestamp()))
        .run(handle.client())
        .await?;
    Ok(())
}

pub async fn finish_failed(handle: &DataHandle, id: i64, error: &str) -> Result<()> {
    LibraryScanRun::where_col(|run| run.id.equal(id))
        .where_col(|run| run.status.equal("running"))
        .set(|run| run.status, "failed".to_string())
        .set(|run| run.finished_at, Some(sqlite_timestamp()))
        .set(|run| run.error_message, Some(error.to_string()))
        .run(handle.client())
        .await?;
    Ok(())
}

pub async fn cancel(handle: &DataHandle, id: i64) -> Result<bool> {
    let updated = LibraryScanRun::where_col(|run| run.id.equal(id))
        .where_col(|run| run.status.equal("running"))
        .set(|run| run.status, "cancelled".to_string())
        .set(|run| run.finished_at, Some(sqlite_timestamp()))
        .run(handle.client())
        .await?;
    Ok(updated == 1)
}

pub async fn is_cancelled(handle: &DataHandle, id: i64) -> Result<bool> {
    Ok(LibraryScanRun::find_by_id(handle.client(), id)
        .await?
        .is_some_and(|run| run.status == "cancelled"))
}

fn summary_from_model(run: welds::state::DbState<LibraryScanRun>) -> LibraryScanRunSummary {
    LibraryScanRunSummary {
        id: run.id,
        status: run.status.clone(),
        files_seen: run.files_seen,
        files_processed: run.files_processed,
        files_indexed: run.files_indexed,
        files_total: run.files_total,
        started_at: run.started_at.clone(),
        finished_at: run.finished_at.clone(),
        error_message: run.error_message.clone(),
    }
}

fn sqlite_timestamp() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

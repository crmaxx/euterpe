use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use euterpe_converter::{ConvertOptions, ConvertProgress, FilePolicy, FlacEncodeSettings};
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, broadcast, mpsc};
use tokio::task::JoinSet;

use crate::api::{ConvertFileProgress, ConvertProgressEvent};
use crate::config::AppConfig;
use crate::db::convert_jobs::{self, ConvertFileStatus, ConvertJobStatus};
use crate::db::tracks;
use crate::error::ApiError;
use crate::library::tags::is_convertible_path;
use crate::services::app_settings;

pub struct ConvertWorkerDeps {
    pub pool: SqlitePool,
    pub config: Arc<AppConfig>,
    pub runtime: app_settings::RuntimeSettingsHandle,
    pub events: broadcast::Sender<ConvertProgressEvent>,
    pub scan_events: broadcast::Sender<crate::api::ScanProgressEvent>,
    pub job_tx: mpsc::Sender<i64>,
}

fn wake(job_tx: &mpsc::Sender<i64>) {
    let _ = job_tx.try_send(0);
}

/// Min interval between DB/SSE flushes during a single-file encode.
const ENCODE_PROGRESS_FLUSH_INTERVAL: Duration = Duration::from_secs(2);
/// Min change in per-file encode % to flush early.
const ENCODE_PROGRESS_MIN_DELTA_PCT: f64 = 2.0;

fn encode_progress_pct(p: ConvertProgress) -> Option<f64> {
    let total = p.pcm_samples_total?;
    if total == 0 {
        return None;
    }
    Some((p.pcm_samples_read as f64 / total as f64) * 100.0)
}

fn files_completed_count(files: &[ConvertFileStatus]) -> i64 {
    files
        .iter()
        .filter(|f| f.status == "success" || f.status == "failed")
        .count() as i64
}

/// Job-level %: average per-file contribution (finished = 100, running = stream %).
fn job_progress_pct(files: &[ConvertFileStatus]) -> f64 {
    if files.is_empty() {
        return 0.0;
    }
    let sum: f64 = files
        .iter()
        .map(|f| match f.status.as_str() {
            "success" | "failed" => 100.0,
            "running" => f.progress_pct.unwrap_or(0.0),
            _ => 0.0,
        })
        .sum();
    sum / files.len() as f64
}

struct EncodeProgressThrottle {
    last_flush: Mutex<Instant>,
    last_pct: Mutex<f64>,
}

impl EncodeProgressThrottle {
    fn new() -> Self {
        Self {
            last_flush: Mutex::new(Instant::now() - ENCODE_PROGRESS_FLUSH_INTERVAL),
            last_pct: Mutex::new(-1.0),
        }
    }

    fn should_flush(&self, new_pct: f64) -> bool {
        let mut last_flush = self.last_flush.lock().expect("throttle lock");
        let mut last_pct = self.last_pct.lock().expect("throttle lock");
        let now = Instant::now();
        let delta = (new_pct - *last_pct).abs();
        if *last_pct < 0.0
            || delta >= ENCODE_PROGRESS_MIN_DELTA_PCT
            || now.duration_since(*last_flush) >= ENCODE_PROGRESS_FLUSH_INTERVAL
        {
            *last_flush = now;
            *last_pct = new_pct;
            return true;
        }
        false
    }
}

async fn persist_convert_snapshot(
    deps: &ConvertWorkerDeps,
    job_id: i64,
    album_id: i64,
    snapshot: &[ConvertFileStatus],
) -> Result<(), ApiError> {
    let payload = serde_json::to_string(snapshot)
        .map_err(|e| ApiError::Message(e.to_string()))?;
    let done = files_completed_count(snapshot);
    let total = snapshot.len() as i64;
    let pct = job_progress_pct(snapshot);
    convert_jobs::update_progress(
        &deps.pool,
        job_id,
        done,
        total,
        pct,
        Some(&payload),
    )
    .await?;
    emit_convert_progress(deps, job_id, album_id, "running", snapshot, None);
    Ok(())
}

pub fn spawn_convert_worker(
    mut job_rx: mpsc::Receiver<i64>,
    deps: ConvertWorkerDeps,
) -> tokio::task::JoinHandle<()> {
    let deps = Arc::new(deps);
    tokio::spawn(async move {
        let _ = try_dispatch(&deps).await;
        while job_rx.recv().await.is_some() {
            if let Err(e) = try_dispatch(&deps).await {
                tracing::error!("convert scheduler dispatch failed: {e}");
            }
        }
    })
}

async fn try_dispatch(deps: &Arc<ConvertWorkerDeps>) -> Result<(), ApiError> {
    loop {
        let Some(id) = convert_jobs::next_queued_id(&deps.pool).await? else {
            break;
        };
        if !convert_jobs::claim_running(&deps.pool, id).await? {
            continue;
        }
        let deps = Arc::clone(deps);
        tokio::spawn(async move {
            if let Err(e) = execute_job(id, &deps).await {
                tracing::error!(job_id = id, "convert job failed: {e}");
                if let Err(finish_err) = mark_job_failed(&deps, id, &e.to_string()).await {
                    tracing::error!(job_id = id, "could not mark convert job failed: {finish_err}");
                }
            }
            wake(&deps.job_tx);
        });
        return Ok(());
    }
    Ok(())
}

async fn execute_job(job_id: i64, deps: &Arc<ConvertWorkerDeps>) -> Result<(), ApiError> {
    let row = convert_jobs::get_by_id(&deps.pool, job_id)
        .await?
        .ok_or_else(|| ApiError::Message(format!("convert job {job_id} not found")))?;

    let settings = deps.runtime.read().await.converter.clone();
    let flac: FlacEncodeSettings = (&settings.flac_encode).into();
    let file_policy: FilePolicy = settings.file_policy.clone().into();

    let track_rows = tracks::list_by_album(&deps.pool, row.album_id).await?;
    let library_root = &deps.config.library_path;
    let mut targets: Vec<(i64, String)> = Vec::new();
    for t in track_rows {
        let rel = Path::new(&t.path);
        if is_convertible_path(rel) {
            targets.push((t.id, t.path));
        }
    }

    if targets.is_empty() {
        convert_jobs::finish(
            &deps.pool,
            job_id,
            ConvertJobStatus::Success,
            None,
            Some("[]"),
        )
        .await?;
        emit_convert_progress(
            deps,
            job_id,
            row.album_id,
            "success",
            &[],
            None,
        );
        return Ok(());
    }

    let parallelism = settings.parallelism.max(1) as usize;
    let sem = Arc::new(Semaphore::new(parallelism));
    let file_statuses: Vec<ConvertFileStatus> = targets
        .iter()
        .map(|(_, p)| ConvertFileStatus {
            path: p.clone(),
            status: "pending".into(),
            progress_pct: None,
            error: None,
        })
        .collect();
    let statuses = Arc::new(Mutex::new(file_statuses));
    let initial_payload = serde_json::to_string(statuses.lock().unwrap().as_slice())
        .map_err(|e| ApiError::Message(e.to_string()))?;
    convert_jobs::update_progress(
        &deps.pool,
        job_id,
        0,
        targets.len() as i64,
        0.0,
        Some(&initial_payload),
    )
    .await?;
    emit_convert_progress(
        deps,
        job_id,
        row.album_id,
        "running",
        &statuses.lock().unwrap(),
        None,
    );

    let mut join_set = JoinSet::new();
    for (idx, (track_id, path_rel)) in targets.into_iter().enumerate() {
        let permit = sem.clone().acquire_owned().await.map_err(|e| {
            ApiError::Message(format!("convert semaphore: {e}"))
        })?;
        let flac = flac.clone();
        let library_root = library_root.clone();
        let statuses = Arc::clone(&statuses);
        let deps_spawn = Arc::clone(deps);
        let album_id = row.album_id;
        join_set.spawn(async move {
            let _permit = permit;
            {
                let mut s = statuses.lock().expect("convert statuses lock");
                if let Some(slot) = s.get_mut(idx) {
                    slot.status = "running".into();
                    slot.progress_pct = Some(0.0);
                }
            }
            emit_convert_progress(
                &deps_spawn,
                job_id,
                album_id,
                "running",
                &statuses.lock().expect("convert statuses lock"),
                None,
            );
            let src = library_root.join(&path_rel);
            let statuses_cb = Arc::clone(&statuses);
            let deps_cb = Arc::clone(&deps_spawn);
            let throttle = Arc::new(EncodeProgressThrottle::new());
            let handle = tokio::runtime::Handle::current();
            let progress: Arc<dyn Fn(ConvertProgress) + Send + Sync> = Arc::new(
                move |p: ConvertProgress| {
                    let file_pct = match encode_progress_pct(p) {
                        Some(v) => v.clamp(0.0, 100.0),
                        None => return,
                    };
                    {
                        let mut s = statuses_cb.lock().expect("convert statuses lock");
                        if let Some(slot) = s.get_mut(idx) {
                            slot.progress_pct = Some(file_pct);
                        }
                    }
                    if !throttle.should_flush(file_pct) {
                        return;
                    }
                    let snap = statuses_cb.lock().expect("convert statuses lock").clone();
                    let deps = Arc::clone(&deps_cb);
                    handle.spawn(async move {
                        if let Err(e) =
                            persist_convert_snapshot(&deps, job_id, album_id, &snap).await
                        {
                            tracing::warn!(
                                job_id,
                                error = %e,
                                "convert encode progress persist failed"
                            );
                        }
                    });
                },
            );
            let convert_result = tokio::task::spawn_blocking(move || {
                euterpe_converter::convert_file(
                    &src,
                    ConvertOptions {
                        flac_encode: &flac,
                        file_policy,
                        on_progress: Some(progress),
                    },
                )
            })
            .await;
            (idx, track_id, path_rel, convert_result)
        });
    }

    let library_root = deps.config.library_path.clone();
    while let Some(res) = join_set.join_next().await {
        match res {
            Ok((idx, track_id, old_rel, task_result)) => match task_result {
                Ok(Ok(converted)) => {
                    let out_path = converted.output_path;
                    let new_rel = out_path
                            .strip_prefix(&library_root)
                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_else(|_| {
                                out_path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or(old_rel.clone())
                            });
                        match tracks::update_path(&deps.pool, track_id, &new_rel).await {
                            Ok(()) => {
                                let mut s = statuses.lock().expect("convert statuses lock");
                                if let Some(slot) = s.get_mut(idx) {
                                    slot.status = "success".into();
                                    slot.path = new_rel;
                                    slot.progress_pct = None;
                                }
                            }
                            Err(e) => {
                                let mut s = statuses.lock().expect("convert statuses lock");
                                if let Some(slot) = s.get_mut(idx) {
                                    slot.status = "failed".into();
                                    slot.error = Some(e.to_string());
                                    slot.progress_pct = None;
                                }
                            }
                        }
                }
                Ok(Err(e)) => {
                    tracing::error!(job_id, path = %old_rel, error = %e, "convert failed");
                    let mut s = statuses.lock().expect("convert statuses lock");
                    if let Some(slot) = s.get_mut(idx) {
                        slot.status = "failed".into();
                        slot.error = Some(e.to_string());
                        slot.progress_pct = None;
                    }
                }
                Err(e) => {
                    tracing::error!(job_id, error = %e, "convert task join failed");
                    let mut s = statuses.lock().expect("convert statuses lock");
                    if let Some(slot) = s.get_mut(idx) {
                        slot.status = "failed".into();
                        slot.error = Some(e.to_string());
                        slot.progress_pct = None;
                    }
                }
            },
            Err(e) => {
                tracing::warn!(job_id, error = %e, "convert join_set failed");
            }
        }
        let snapshot = statuses.lock().expect("convert statuses lock").clone();
        persist_convert_snapshot(deps, job_id, row.album_id, &snapshot).await?;
    }

    let snapshot = statuses.lock().expect("convert statuses lock").clone();
    let all_ok = snapshot.iter().all(|f| f.status == "success");
    let payload = serde_json::to_string(&snapshot)
        .map_err(|e| ApiError::Message(e.to_string()))?;
    let status = if all_ok {
        ConvertJobStatus::Success
    } else {
        ConvertJobStatus::Failed
    };
    let err_msg = if all_ok {
        None
    } else {
        Some("one or more files failed to convert".to_string())
    };
    convert_jobs::finish(
        &deps.pool,
        job_id,
        status,
        err_msg.as_deref(),
        Some(&payload),
    )
    .await?;

    emit_convert_progress(
        deps,
        job_id,
        row.album_id,
        status.as_str(),
        &snapshot,
        err_msg.clone(),
    );

    if let Some(album) = crate::db::albums::get_by_id(&deps.pool, row.album_id).await?
        && let Some(ref album_path) = album.path
    {
        let scan_root =
            crate::services::library_scan::resolve_scan_root_query(
                &deps.config.library_path,
                Some(album_path.as_str()),
            )?;
        let scan_cfg = deps
            .runtime
            .read()
            .await
            .library_scan_config(deps.config.debug)?;
        let _ = crate::services::library_scan::start_scan(
            &deps.pool,
            deps.config.library_path.clone(),
            deps.scan_events.clone(),
            scan_cfg,
            scan_root,
            Some(deps.job_tx.clone()),
            Some(deps.runtime.clone()),
        )
        .await;
    }

    Ok(())
}

async fn mark_job_failed(
    deps: &ConvertWorkerDeps,
    job_id: i64,
    message: &str,
) -> Result<(), ApiError> {
    let row = convert_jobs::get_by_id(&deps.pool, job_id)
        .await?
        .ok_or_else(|| ApiError::Message(format!("convert job {job_id} not found")))?;
    convert_jobs::finish(
        &deps.pool,
        job_id,
        ConvertJobStatus::Failed,
        Some(message),
        None,
    )
    .await?;
    emit_convert_progress(
        deps,
        job_id,
        row.album_id,
        "failed",
        &[],
        Some(message.to_string()),
    );
    Ok(())
}

fn emit_progress(deps: &ConvertWorkerDeps, event: ConvertProgressEvent) {
    let _ = deps.events.send(event);
}

fn emit_convert_progress(
    deps: &ConvertWorkerDeps,
    job_id: i64,
    album_id: i64,
    status: &str,
    files: &[ConvertFileStatus],
    error_message: Option<String>,
) {
    let total = files.len() as i64;
    let done = files
        .iter()
        .filter(|f| f.status == "success" || f.status == "failed")
        .count() as i64;
    let progress_pct = if total == 0 {
        if status == "success" { 100.0 } else { 0.0 }
    } else {
        job_progress_pct(files)
    };
    emit_progress(
        deps,
        ConvertProgressEvent {
            job_id,
            album_id,
            status: status.into(),
            files_total: total,
            files_done: done,
            progress_pct,
            files: files
                .iter()
                .map(|f| ConvertFileProgress {
                    path: f.path.clone(),
                    status: f.status.clone(),
                    progress_pct: f.progress_pct,
                    error: f.error.clone(),
                })
                .collect(),
            error_message,
        },
    );
}

pub async fn start_album_convert(
    pool: &SqlitePool,
    album_id: i64,
    job_tx: &mpsc::Sender<i64>,
) -> Result<i64, ApiError> {
    if convert_jobs::album_has_active_job(pool, album_id).await? {
        return Err(ApiError::bad_request("convert job already running for album"));
    }
    let track_rows = tracks::list_by_album(pool, album_id).await?;
    let convertible = track_rows
        .iter()
        .filter(|t| is_convertible_path(Path::new(&t.path)))
        .count() as i64;
    if convertible == 0 {
        return Err(ApiError::bad_request("no convertible tracks in album"));
    }
    let id = convert_jobs::create(pool, album_id, convert_jobs::ConvertTrigger::Manual, convertible)
        .await?;
    wake(job_tx);
    Ok(id)
}

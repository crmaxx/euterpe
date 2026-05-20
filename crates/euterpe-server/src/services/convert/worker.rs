use std::path::Path;
use std::sync::Arc;

use euterpe_converter::{ConvertOptions, FilePolicy, FlacEncodeSettings};
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, broadcast, mpsc};
use tokio::task::JoinSet;

use crate::api::ConvertProgressEvent;
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
        emit_progress(
            deps,
            ConvertProgressEvent {
                job_id,
                album_id: row.album_id,
                status: "success".into(),
                files_total: 0,
                files_done: 0,
                progress_pct: 100.0,
                error_message: None,
            },
        );
        return Ok(());
    }

    convert_jobs::update_progress(&deps.pool, job_id, 0, targets.len() as i64, Some("[]"))
        .await?;

    let parallelism = settings.parallelism.max(1) as usize;
    let sem = Arc::new(Semaphore::new(parallelism));
    let mut file_statuses: Vec<ConvertFileStatus> = targets
        .iter()
        .map(|(_, p)| ConvertFileStatus {
            path: p.clone(),
            status: "pending".into(),
            error: None,
        })
        .collect();

    let mut join_set = JoinSet::new();
    for (idx, (track_id, path_rel)) in targets.into_iter().enumerate() {
        let permit = sem.clone().acquire_owned().await.map_err(|e| {
            ApiError::Message(format!("convert semaphore: {e}"))
        })?;
        let flac = flac.clone();
        let library_root = library_root.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let src = library_root.join(&path_rel);
            let convert_result = tokio::task::spawn_blocking(move || {
                euterpe_converter::convert_file(
                    &src,
                    ConvertOptions {
                        flac_encode: &flac,
                        file_policy,
                    },
                )
            })
            .await;
            (idx, track_id, path_rel, convert_result)
        });
    }

    let mut done = 0i64;
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
                                if let Some(slot) = file_statuses.get_mut(idx) {
                                    slot.status = "success".into();
                                }
                            }
                            Err(e) => {
                                if let Some(slot) = file_statuses.get_mut(idx) {
                                    slot.status = "failed".into();
                                    slot.error = Some(e.to_string());
                                }
                            }
                        }
                }
                Ok(Err(e)) => {
                    tracing::warn!(job_id, path = %old_rel, error = %e, "convert failed");
                    if let Some(slot) = file_statuses.get_mut(idx) {
                        slot.status = "failed".into();
                        slot.error = Some(e.to_string());
                    }
                }
                Err(e) => {
                    tracing::warn!(job_id, error = %e, "convert task join failed");
                    if let Some(slot) = file_statuses.get_mut(idx) {
                        slot.status = "failed".into();
                        slot.error = Some(e.to_string());
                    }
                }
            },
            Err(e) => {
                tracing::warn!(job_id, error = %e, "convert join_set failed");
            }
        }
        done += 1;
        let payload = serde_json::to_string(&file_statuses)
            .map_err(|e| ApiError::Message(e.to_string()))?;
        convert_jobs::update_progress(
            &deps.pool,
            job_id,
            done,
            file_statuses.len() as i64,
            Some(&payload),
        )
        .await?;
        emit_progress(
            deps,
            ConvertProgressEvent {
                job_id,
                album_id: row.album_id,
                status: "running".into(),
                files_total: file_statuses.len() as i64,
                files_done: done,
                progress_pct: if file_statuses.is_empty() {
                    0.0
                } else {
                    (done as f64 / file_statuses.len() as f64) * 100.0
                },
                error_message: None,
            },
        );
    }

    let all_ok = file_statuses.iter().all(|f| f.status == "success");
    let payload = serde_json::to_string(&file_statuses)
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

    emit_progress(
        deps,
        ConvertProgressEvent {
            job_id,
            album_id: row.album_id,
            status: status.as_str().into(),
            files_total: file_statuses.len() as i64,
            files_done: done,
            progress_pct: if all_ok { 100.0 } else { 0.0 },
            error_message: err_msg.clone(),
        },
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

fn emit_progress(deps: &ConvertWorkerDeps, event: ConvertProgressEvent) {
    let _ = deps.events.send(event);
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

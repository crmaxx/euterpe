use std::path::PathBuf;
use std::time::Duration;

use euterpe_torrent::StartJobRequest;
use tokio::time::interval;

use crate::api::{JobProgressEvent, TorrentEuterpePhase};
use crate::db::download_jobs;
use crate::error::ApiError;
use crate::library::storage::{self, StoragePath};
use crate::services::download::WorkerDeps;
use crate::services::download::payload::{DownloadJobPayload, TorrentRuntimeSnapshot};
use crate::services::library_scan;
use crate::services::torrent_import;
use crate::services::torrent_settings;

async fn persist_torrent_runtime(
    deps: &WorkerDeps,
    job_id: i64,
    snapshot: &TorrentRuntimeSnapshot,
    progress_pct: f64,
    download_speed_bps: u64,
) -> Result<(), ApiError> {
    let mut payload = download_jobs::get_payload(&deps.pool, job_id)
        .await?
        .unwrap_or_default();
    payload.set_torrent_runtime(snapshot.clone());
    download_jobs::set_payload(&deps.pool, job_id, &payload).await?;
    download_jobs::update_progress_and_speed(
        &deps.pool,
        job_id,
        progress_pct,
        Some(download_speed_bps),
    )
    .await?;
    Ok(())
}

async fn emit_torrent_progress(
    deps: &WorkerDeps,
    job_id: i64,
    snapshot: &TorrentRuntimeSnapshot,
    progress_pct: f64,
    download_speed_bps: u64,
) -> Result<(), ApiError> {
    persist_torrent_runtime(deps, job_id, snapshot, progress_pct, download_speed_bps).await?;
    let detail = snapshot.to_api_detail();
    let _ = deps.events.send(JobProgressEvent {
        id: job_id,
        progress_pct,
        download_speed_bps,
        torrent_detail: Some(detail),
    });
    Ok(())
}

async fn emit_from_stats(
    deps: &WorkerDeps,
    job_id: i64,
    stats: &euterpe_torrent::JobStats,
    phase: Option<TorrentEuterpePhase>,
) -> Result<(), ApiError> {
    let snapshot = TorrentRuntimeSnapshot::from_job_stats(stats, phase);
    emit_torrent_progress(
        deps,
        job_id,
        &snapshot,
        stats.progress_pct,
        stats.download_speed_bps,
    )
    .await
}

pub async fn run_torrent_job(job_id: i64, deps: &WorkerDeps) -> Result<(), ApiError> {
    let _permit = match &deps.torrent_semaphore {
        Some(sem) => Some(
            sem.acquire()
                .await
                .map_err(|e| ApiError::Message(e.to_string()))?,
        ),
        None => None,
    };

    let torrent = deps
        .torrent
        .as_ref()
        .ok_or_else(|| ApiError::Message("torrent engine not configured".into()))?;

    let mut payload = download_jobs::get_payload(&deps.pool, job_id)
        .await?
        .and_then(|p| p.torrent)
        .ok_or_else(|| ApiError::Message(format!("job {job_id} missing torrent payload")))?;

    let save_dir = PathBuf::from(&payload.save_dir_incoming);
    let settings = torrent_settings::load(&deps.pool).await?;
    let limits = torrent_settings::to_limits_config(&settings);

    let torrent_bytes = if payload.magnet.is_none() {
        let path = save_dir.join("seed.torrent");
        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| ApiError::Message(format!("read {}: {e}", path.display())))?;
        Some(bytes::Bytes::from(data))
    } else {
        None
    };

    let start_req = StartJobRequest {
        magnet: payload.magnet.clone(),
        torrent_bytes,
        only_files: payload.selected_file_indices.clone(),
        output_folder: save_dir.clone(),
        ratelimits: limits,
    };

    let connecting = TorrentRuntimeSnapshot::connecting();
    emit_torrent_progress(deps, job_id, &connecting, 0.0, 0).await?;
    tracing::info!(
        job_id,
        magnet = payload.magnet.is_some(),
        files = payload.selected_file_indices.len(),
        dir = %save_dir.display(),
        "torrent job: starting librqbit session"
    );

    let handle = torrent
        .start_job(start_req)
        .await
        .map_err(map_torrent_err)?;

    payload.librqbit_id = Some(handle.librqbit_id);
    let wrapped = DownloadJobPayload {
        torrent: Some(payload.clone()),
        ..Default::default()
    };
    download_jobs::set_payload(&deps.pool, job_id, &wrapped).await?;

    let poll_stats = || async {
        let stats = torrent.job_stats(&handle).await.map_err(map_torrent_err)?;
        emit_from_stats(deps, job_id, &stats, Some(TorrentEuterpePhase::Downloading)).await?;
        Ok::<_, ApiError>(stats)
    };

    let mut stats = poll_stats().await?;
    if stats.finished {
        // already done (e.g. tiny torrent)
    } else {
        let mut ticker = interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            if download_jobs::is_stopped(&deps.pool, job_id).await? {
                let _ = torrent.cancel(&handle).await;
                if download_jobs::is_cancelled(&deps.pool, job_id).await? {
                    let _ = tokio::fs::remove_dir_all(&save_dir).await;
                }
                return Ok(());
            }
            stats = poll_stats().await?;
            if stats.finished {
                break;
            }
        }
    }

    torrent
        .wait_until_completed(&handle)
        .await
        .map_err(map_torrent_err)?;

    if download_jobs::is_stopped(&deps.pool, job_id).await? {
        let _ = torrent.cancel(&handle).await;
        if download_jobs::is_cancelled(&deps.pool, job_id).await? {
            let _ = tokio::fs::remove_dir_all(&save_dir).await;
        }
        return Ok(());
    }

    if payload.copy_to_library {
        let finished_stats = torrent.job_stats(&handle).await.map_err(map_torrent_err)?;
        emit_from_stats(
            deps,
            job_id,
            &finished_stats,
            Some(TorrentEuterpePhase::Importing),
        )
        .await?;

        let storage_location = deps
            .runtime
            .read()
            .await
            .storage
            .library
            .clone()
            .ok_or_else(|| {
                ApiError::Message(
                    "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
                )
            })?;
        let storage =
            storage::storage_from_location(&storage_location, deps.config.master_key.as_ref())?;
        let rel = torrent_import::copy_to_library_storage(
            &save_dir,
            storage.as_ref(),
            &payload.display_name,
        )
        .await?;
        payload.library_dest_rel = Some(rel.clone());
        let wrapped = DownloadJobPayload {
            torrent: Some(payload.clone()),
            ..Default::default()
        };
        download_jobs::set_payload(&deps.pool, job_id, &wrapped).await?;

        let post_processing_requested = payload.post_download.as_ref().is_some_and(|post| {
            post.convert_after_download || post.split_after_download || post.split_after_conversion
        });
        if post_processing_requested {
            tracing::warn!(
                job_id,
                library_dest_rel = %rel,
                "torrent post-download CUE/convert is deferred until storage-native CUE/convert jobs"
            );
        }

        if payload.auto_index_after_import || post_processing_requested {
            let scan_cfg = deps
                .runtime
                .read()
                .await
                .library_scan_config(deps.config.debug)?;
            let _ = library_scan::start_scan_storage(
                &deps.pool,
                storage.clone(),
                deps.scan_events.clone(),
                scan_cfg,
                Some(StoragePath::parse(&rel)?),
                None,
                Some(deps.runtime.clone()),
            )
            .await?;
        }

        let _ = tokio::fs::remove_dir_all(&save_dir).await;
    }

    torrent
        .remove_from_session(&handle)
        .await
        .map_err(map_torrent_err)?;

    if download_jobs::is_stopped(&deps.pool, job_id).await? {
        return Ok(());
    }

    download_jobs::finish_success(&deps.pool, job_id).await?;
    let _ = deps.events.send(JobProgressEvent {
        id: job_id,
        progress_pct: 100.0,
        download_speed_bps: 0,
        torrent_detail: None,
    });
    Ok(())
}

pub fn map_torrent_err(e: euterpe_torrent::TorrentError) -> ApiError {
    ApiError::Message(e.to_string())
}

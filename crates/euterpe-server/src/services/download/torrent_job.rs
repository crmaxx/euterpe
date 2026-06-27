use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use euterpe_data::DataHandle;
use euterpe_data::repositories::{
    catalog, convert_jobs, cue_jobs, download_jobs, library_scan_runs,
};
use euterpe_torrent::StartJobRequest;
use tokio::time::interval;

use crate::api::{
    CueSplitRequest, JobProgressEvent, TorrentEuterpePhase, TorrentPostDownloadOptions,
};
use crate::error::ApiError;
use crate::library::cue;
use crate::library::storage::{self, StoragePath};
use crate::services::convert::start_album_convert;
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
    let mut payload: DownloadJobPayload = download_jobs::get_payload(&deps.data, job_id)
        .await?
        .unwrap_or_default();
    payload.set_torrent_runtime(snapshot.clone());
    download_jobs::set_payload(&deps.data, job_id, &payload).await?;
    download_jobs::update_progress_and_speed(
        &deps.data,
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

    let mut payload = download_jobs::get_payload::<DownloadJobPayload>(&deps.data, job_id)
        .await?
        .and_then(|p| p.torrent)
        .ok_or_else(|| ApiError::Message(format!("job {job_id} missing torrent payload")))?;

    let save_dir = PathBuf::from(&payload.save_dir_incoming);
    let settings = torrent_settings::load(&deps.data).await?;
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
    download_jobs::set_payload(&deps.data, job_id, &wrapped).await?;

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
            if download_jobs::is_stopped(&deps.data, job_id).await? {
                let _ = torrent.cancel(&handle).await;
                if download_jobs::is_cancelled(&deps.data, job_id).await? {
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

    if download_jobs::is_stopped(&deps.data, job_id).await? {
        let _ = torrent.cancel(&handle).await;
        if download_jobs::is_cancelled(&deps.data, job_id).await? {
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
        let import_cancelled = Arc::new(AtomicBool::new(false));
        let monitor = spawn_job_stop_monitor(
            deps.data.clone(),
            job_id,
            Arc::clone(&import_cancelled),
            Duration::from_millis(200),
        );
        let copy_result = torrent_import::copy_to_library_storage_cancellable(
            &save_dir,
            storage.as_ref(),
            &payload.display_name,
            Arc::new({
                let import_cancelled = Arc::clone(&import_cancelled);
                move || import_cancelled.load(Ordering::SeqCst)
            }),
        )
        .await;
        monitor.abort();
        if download_jobs::is_stopped(&deps.data, job_id).await? {
            cleanup_cancelled_incoming(&deps.data, job_id, &save_dir).await?;
            return Ok(());
        }
        let rel = copy_result?;
        payload.library_dest_rel = Some(rel.clone());
        let wrapped = DownloadJobPayload {
            torrent: Some(payload.clone()),
            ..Default::default()
        };
        download_jobs::set_payload(&deps.data, job_id, &wrapped).await?;

        if payload.auto_index_after_import || payload.post_download.is_some() {
            let scan_cfg = deps
                .runtime
                .read()
                .await
                .library_scan_config(deps.config.debug)?;
            let scan_id = library_scan::start_scan_storage(
                &deps.data,
                storage.clone(),
                deps.scan_events.clone(),
                scan_cfg,
                Some(StoragePath::parse(&rel)?),
                Some(deps.convert_job_tx.clone()),
                Some(deps.runtime.clone()),
            )
            .await?;
            if wait_scan_finished_or_stopped(&deps.data, job_id, scan_id).await? {
                cleanup_cancelled_incoming(&deps.data, job_id, &save_dir).await?;
                return Ok(());
            }
        }

        if download_jobs::is_stopped(&deps.data, job_id).await? {
            cleanup_cancelled_incoming(&deps.data, job_id, &save_dir).await?;
            return Ok(());
        }
        if let Some(post) = &payload.post_download {
            run_torrent_post_download(job_id, deps, &rel, post).await?;
        }

        let _ = tokio::fs::remove_dir_all(&save_dir).await;
    }

    torrent
        .remove_from_session(&handle)
        .await
        .map_err(map_torrent_err)?;

    if download_jobs::is_stopped(&deps.data, job_id).await? {
        return Ok(());
    }

    download_jobs::finish_success(&deps.data, job_id).await?;
    let _ = deps.events.send(JobProgressEvent {
        id: job_id,
        progress_pct: 100.0,
        download_speed_bps: 0,
        torrent_detail: None,
    });
    Ok(())
}

async fn run_torrent_post_download(
    job_id: i64,
    deps: &WorkerDeps,
    library_dest_rel: &str,
    post: &TorrentPostDownloadOptions,
) -> Result<(), ApiError> {
    if download_jobs::is_stopped(&deps.data, job_id).await? {
        return Ok(());
    }

    let mut convert_job_id = None;
    if post.convert_after_download {
        let album_id = imported_album_id(&deps.data, library_dest_rel, post).await?;
        if download_jobs::is_stopped(&deps.data, job_id).await? {
            return Ok(());
        }
        let queued_convert_job_id =
            start_album_convert(&deps.data, album_id, &deps.convert_job_tx).await?;
        convert_job_id = Some(queued_convert_job_id);
        tracing::info!(
            job_id,
            convert_job_id = queued_convert_job_id,
            album_id,
            library_dest_rel,
            "torrent post-download conversion queued"
        );
    }

    if post.split_after_download || post.split_after_conversion {
        if download_jobs::is_stopped(&deps.data, job_id).await? {
            return Ok(());
        }
        if post.split_after_conversion {
            if !post.convert_after_download {
                return Err(ApiError::bad_request(
                    "split_after_conversion requires convert_after_download",
                ));
            }
            let convert_job_id = convert_job_id.ok_or_else(|| {
                ApiError::Message("torrent post-download conversion job was not queued".into())
            })?;
            if wait_convert_finished_or_stopped(&deps.data, job_id, convert_job_id).await? {
                return Ok(());
            }
            if download_jobs::is_stopped(&deps.data, job_id).await? {
                return Ok(());
            }
        }
        run_torrent_cue_split_after_download(job_id, deps, library_dest_rel, post).await?;
    }

    Ok(())
}

async fn run_torrent_cue_split_after_download(
    job_id: i64,
    deps: &WorkerDeps,
    library_dest_rel: &str,
    post: &TorrentPostDownloadOptions,
) -> Result<(), ApiError> {
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
    let cue_rel = torrent_post_cue_path(library_dest_rel, post)?;
    storage.metadata(&cue_rel).await?;
    let cue_album_rel = cue_rel.parent().unwrap_or_else(|| {
        StoragePath::parse(library_dest_rel).unwrap_or_else(|_| StoragePath::root())
    });
    let album_id = imported_album_id(&deps.data, library_dest_rel, post).await?;
    let loaded = cue::load_album_cue_storage(
        storage.as_ref(),
        cue_album_rel.as_str(),
        Some(cue_rel.as_str()),
    )
    .await?;
    if !loaded.validation.valid {
        return Err(ApiError::bad_request("CUE has validation errors"));
    }
    let source_file_policy = post
        .source_file_policy
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("keep");
    if !matches!(source_file_policy, "keep" | "delete_after_success") {
        return Err(ApiError::bad_request("invalid source_file_policy"));
    }
    let payload = cue_jobs::CueJobPayload {
        cue_path: loaded.document.cue_path.clone(),
        audio_path: loaded.document.audio_path.clone(),
        source_file_policy: source_file_policy.to_string(),
    };
    let tracks_total = loaded
        .document
        .tracks
        .iter()
        .filter(|track| track.selected)
        .count() as i64;
    let cue_job_id =
        cue_jobs::create_queued(&deps.data, album_id, tracks_total, Some(&payload)).await?;
    if download_jobs::is_stopped(&deps.data, job_id).await? {
        cue_jobs::finish_failed(
            &deps.data,
            cue_job_id,
            "torrent job stopped before CUE split",
        )
        .await?;
        return Ok(());
    }
    let split_cancelled = Arc::new(AtomicBool::new(false));
    let monitor = spawn_job_stop_monitor(
        deps.data.clone(),
        job_id,
        Arc::clone(&split_cancelled),
        Duration::from_millis(200),
    );
    let split_result = cue::run_storage_cue_split_job(
        &deps.data,
        storage.clone(),
        cue_job_id,
        CueSplitRequest {
            document: loaded.document,
            source_file_policy: source_file_policy.to_string(),
            file_mask: None,
        },
        Some(Arc::new({
            let split_cancelled = Arc::clone(&split_cancelled);
            move || split_cancelled.load(Ordering::SeqCst)
        })),
    )
    .await;
    monitor.abort();
    split_result?;
    if download_jobs::is_stopped(&deps.data, job_id).await? {
        return Ok(());
    }

    let scan_cfg = deps
        .runtime
        .read()
        .await
        .library_scan_config(deps.config.debug)?;
    let scan_id = library_scan::start_scan_storage(
        &deps.data,
        storage,
        deps.scan_events.clone(),
        scan_cfg,
        Some(cue_album_rel),
        Some(deps.convert_job_tx.clone()),
        Some(deps.runtime.clone()),
    )
    .await?;
    let _ = wait_scan_finished_or_stopped(&deps.data, job_id, scan_id).await?;
    Ok(())
}

fn torrent_post_cue_path(
    library_dest_rel: &str,
    post: &TorrentPostDownloadOptions,
) -> Result<StoragePath, ApiError> {
    let cue_path = post
        .cue_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| ApiError::bad_request("cue_path is required for torrent CUE split"))?;
    if !std::path::Path::new(cue_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cue"))
    {
        return Err(ApiError::bad_request("cue_path must point to a .cue file"));
    }
    let dest = StoragePath::parse(library_dest_rel)?;
    let cue_rel = dest.join(cue_path)?;
    Ok(cue_rel)
}

fn spawn_job_stop_monitor(
    data: DataHandle,
    job_id: i64,
    flag: Arc<AtomicBool>,
    period: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(period);
        loop {
            ticker.tick().await;
            match download_jobs::is_stopped(&data, job_id).await {
                Ok(true) => {
                    flag.store(true, Ordering::SeqCst);
                    break;
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(job_id, error = %e, "torrent import cancellation monitor failed");
                    break;
                }
            }
        }
    })
}

async fn cleanup_cancelled_incoming(
    data: &DataHandle,
    job_id: i64,
    save_dir: &PathBuf,
) -> Result<(), ApiError> {
    if download_jobs::is_cancelled(data, job_id).await? {
        let _ = tokio::fs::remove_dir_all(save_dir).await;
    }
    Ok(())
}

async fn wait_scan_finished_or_stopped(
    data: &DataHandle,
    job_id: i64,
    scan_id: i64,
) -> Result<bool, ApiError> {
    loop {
        if download_jobs::is_stopped(data, job_id).await? {
            return Ok(true);
        }
        match library_scan_runs::get_by_id(data, scan_id).await {
            Ok(Some(run)) => match run.status.as_str() {
                "running" => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                "success" => return Ok(false),
                "failed" => {
                    let error = run
                        .error_message
                        .unwrap_or_else(|| "library scan failed".into());
                    return Err(ApiError::Message(format!(
                        "torrent post-import scan failed: {error}"
                    )));
                }
                "cancelled" => {
                    return Err(ApiError::Message(
                        "torrent post-import scan cancelled".into(),
                    ));
                }
                other => {
                    return Err(ApiError::Message(format!(
                        "torrent post-import scan ended with unsupported status: {other}"
                    )));
                }
            },
            Ok(None) => {
                return Err(ApiError::Message(format!("scan {scan_id} not found")));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn wait_convert_finished_or_stopped(
    data: &DataHandle,
    job_id: i64,
    convert_job_id: i64,
) -> Result<bool, ApiError> {
    loop {
        if download_jobs::is_stopped(data, job_id).await? {
            return Ok(true);
        }
        let row = convert_jobs::get_by_id(data, convert_job_id)
            .await?
            .ok_or_else(|| ApiError::Message(format!("convert job {convert_job_id} not found")))?;
        match row.status.as_str() {
            "queued" | "running" => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            "success" => return Ok(false),
            "failed" => {
                let error = row
                    .error_message
                    .unwrap_or_else(|| "one or more files failed to convert".into());
                return Err(ApiError::Message(format!(
                    "torrent post-download conversion failed: {error}"
                )));
            }
            "cancelled" => {
                return Err(ApiError::Message(
                    "torrent post-download conversion cancelled".into(),
                ));
            }
            other => {
                return Err(ApiError::Message(format!(
                    "torrent post-download conversion ended with unsupported status: {other}"
                )));
            }
        }
    }
}

async fn imported_album_id(
    data: &DataHandle,
    library_dest_rel: &str,
    post: &TorrentPostDownloadOptions,
) -> Result<i64, ApiError> {
    let dest = StoragePath::parse(library_dest_rel)?;
    let mut candidates = Vec::new();
    if let Some(cue_path) = post
        .cue_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        let cue_rel = dest.join(cue_path)?;
        candidates.push(cue_rel.parent().unwrap_or_else(|| dest.clone()));
    }
    candidates.push(dest.clone());

    for candidate in candidates {
        if let Some(album_id) = catalog::album_id_by_path(data, candidate.as_str()).await? {
            return Ok(album_id);
        }
    }

    catalog::album_id_by_path_or_prefix(data, dest.as_str())
        .await?
        .ok_or_else(|| {
            ApiError::Message(format!(
                "torrent post-download album not found after import scan: {library_dest_rel}"
            ))
        })
}

pub fn map_torrent_err(e: euterpe_torrent::TorrentError) -> ApiError {
    ApiError::Message(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::DownloadJobType;
    use crate::test_db::{
        albums, artists, connect, convert_jobs, download_jobs, library_scan_runs, migrate, tracks,
    };
    use std::sync::atomic::AtomicUsize;

    static ALBUM_SEQ: AtomicUsize = AtomicUsize::new(0);

    fn write_test_wav(path: &std::path::Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for sample in 0..(44_100 * 2) {
            writer.write_sample((sample % 1024) as i16).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn make_flac_image(dir: &std::path::Path) {
        let wav = dir.join("album.wav");
        write_test_wav(&wav);
        euterpe_converter::convert_file(
            &wav,
            euterpe_converter::ConvertOptions {
                flac_encode: &euterpe_converter::FlacEncodeSettings::default(),
                file_policy: euterpe_converter::FilePolicy::SiblingThenDelete,
                on_progress: None,
            },
        )
        .unwrap();
    }

    fn worker_deps_from_state(state: &crate::AppState) -> WorkerDeps {
        WorkerDeps {
            data: state.data.clone(),
            qobuz: Arc::clone(&state.qobuz),
            config: Arc::clone(&state.config),
            runtime: state.runtime.clone(),
            events: state.events.clone(),
            http: state.http.clone(),
            torrent: None,
            torrent_semaphore: None,
            scan_events: state.scan_events.clone(),
            job_tx: state.job_tx.clone(),
            convert_job_tx: state.convert_job_tx.clone(),
        }
    }

    async fn seed_torrent_cue_album() -> (crate::AppState, std::path::PathBuf, String) {
        let state = crate::app::test_support::test_state_without_worker().await;
        let album_rel = format!(
            "Torrent Artist/Torrent Cue Album {}",
            ALBUM_SEQ.fetch_add(1, Ordering::SeqCst)
        );
        let album_dir = state.config.library_path.join(&album_rel);
        std::fs::create_dir_all(&album_dir).unwrap();
        make_flac_image(&album_dir);
        std::fs::write(
            album_dir.join("album.cue"),
            r#"
REM GENRE "Rock"
REM DATE 1972
PERFORMER "Torrent Artist"
TITLE "Torrent Cue Album"
FILE "album.flac" FLAC
  TRACK 01 AUDIO
    TITLE "One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Two"
    INDEX 01 00:01:00
"#,
        )
        .unwrap();
        let artist_id = artists::upsert_by_name(&state.data.sqlx_pool(), "Torrent Artist", None)
            .await
            .unwrap();
        let album_id = albums::upsert(
            &state.data.sqlx_pool(),
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Torrent Cue Album",
                year: Some(1972),
                qobuz_album_id: None,
                path: Some(&album_rel),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        tracks::upsert(
            &state.data.sqlx_pool(),
            tracks::TrackUpsert {
                album_id,
                title: "Convertible source",
                track_number: Some(1),
                year: Some(1972),
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: &format!("{album_rel}/album.wav"),
                duration_sec: Some(120),
                file_mtime: None,
                file_hash: None,
                file_size: Some(1024),
            },
        )
        .await
        .unwrap();
        (state, album_dir, album_rel)
    }

    #[tokio::test]
    async fn scan_wait_returns_when_torrent_job_is_cancelled() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(&pool, DownloadJobType::Torrent, 0, 0, None)
            .await
            .unwrap();
        assert!(download_jobs::claim_running(&pool, job_id).await.unwrap());
        let scan_id = library_scan_runs::start(&pool).await.unwrap();

        download_jobs::cancel(&pool, job_id).await.unwrap();
        let stopped = wait_scan_finished_or_stopped(
            &DataHandle::from_sqlite_pool(pool.clone()),
            job_id,
            scan_id,
        )
        .await
        .unwrap();

        assert!(stopped);
    }

    #[tokio::test]
    async fn scan_wait_returns_when_scan_succeeds() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(&pool, DownloadJobType::Torrent, 0, 0, None)
            .await
            .unwrap();
        assert!(download_jobs::claim_running(&pool, job_id).await.unwrap());
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        library_scan_runs::finish_success(&pool, scan_id)
            .await
            .unwrap();

        let stopped = wait_scan_finished_or_stopped(
            &DataHandle::from_sqlite_pool(pool.clone()),
            job_id,
            scan_id,
        )
        .await
        .unwrap();

        assert!(!stopped);
    }

    #[tokio::test]
    async fn scan_wait_errors_when_scan_fails() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(&pool, DownloadJobType::Torrent, 0, 0, None)
            .await
            .unwrap();
        assert!(download_jobs::claim_running(&pool, job_id).await.unwrap());
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        library_scan_runs::finish_failed(&pool, scan_id, "scanner exploded")
            .await
            .unwrap();

        let error = wait_scan_finished_or_stopped(
            &DataHandle::from_sqlite_pool(pool.clone()),
            job_id,
            scan_id,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("scanner exploded"));
    }

    #[tokio::test]
    async fn scan_wait_errors_when_scan_is_cancelled() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(&pool, DownloadJobType::Torrent, 0, 0, None)
            .await
            .unwrap();
        assert!(download_jobs::claim_running(&pool, job_id).await.unwrap());
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        library_scan_runs::cancel(&pool, scan_id).await.unwrap();

        let error = wait_scan_finished_or_stopped(
            &DataHandle::from_sqlite_pool(pool.clone()),
            job_id,
            scan_id,
        )
        .await
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("torrent post-import scan cancelled")
        );
    }

    #[tokio::test]
    async fn scan_wait_errors_when_scan_disappears() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(&pool, DownloadJobType::Torrent, 0, 0, None)
            .await
            .unwrap();
        assert!(download_jobs::claim_running(&pool, job_id).await.unwrap());

        let error =
            wait_scan_finished_or_stopped(&DataHandle::from_sqlite_pool(pool.clone()), job_id, 999)
                .await
                .unwrap_err();

        assert!(error.to_string().contains("scan 999 not found"));
    }

    #[tokio::test]
    async fn conversion_wait_returns_when_convert_job_succeeds() {
        let (state, _, album_rel) = seed_torrent_cue_album().await;
        let job_id = download_jobs::insert_queued(
            &state.data.sqlx_pool(),
            DownloadJobType::Torrent,
            0,
            0,
            None,
        )
        .await
        .unwrap();
        assert!(
            download_jobs::claim_running(&state.data.sqlx_pool(), job_id)
                .await
                .unwrap()
        );
        let album_id = albums::id_by_path(&state.data.sqlx_pool(), &album_rel)
            .await
            .unwrap()
            .unwrap();
        let convert_job_id = convert_jobs::create(
            &state.data.sqlx_pool(),
            album_id,
            convert_jobs::ConvertTrigger::Manual,
            1,
        )
        .await
        .unwrap();
        convert_jobs::finish(
            &state.data.sqlx_pool(),
            convert_job_id,
            convert_jobs::ConvertJobStatus::Success,
            None,
            None,
        )
        .await
        .unwrap();

        let stopped = wait_convert_finished_or_stopped(&state.data, job_id, convert_job_id)
            .await
            .unwrap();

        assert!(!stopped);
    }

    #[tokio::test]
    async fn conversion_wait_errors_when_convert_job_fails() {
        let (state, _, album_rel) = seed_torrent_cue_album().await;
        let job_id = download_jobs::insert_queued(
            &state.data.sqlx_pool(),
            DownloadJobType::Torrent,
            0,
            0,
            None,
        )
        .await
        .unwrap();
        assert!(
            download_jobs::claim_running(&state.data.sqlx_pool(), job_id)
                .await
                .unwrap()
        );
        let album_id = albums::id_by_path(&state.data.sqlx_pool(), &album_rel)
            .await
            .unwrap()
            .unwrap();
        let convert_job_id = convert_jobs::create(
            &state.data.sqlx_pool(),
            album_id,
            convert_jobs::ConvertTrigger::Manual,
            1,
        )
        .await
        .unwrap();
        convert_jobs::finish(
            &state.data.sqlx_pool(),
            convert_job_id,
            convert_jobs::ConvertJobStatus::Failed,
            Some("encode exploded"),
            None,
        )
        .await
        .unwrap();

        let error = wait_convert_finished_or_stopped(&state.data, job_id, convert_job_id)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("encode exploded"));
    }

    #[tokio::test]
    async fn conversion_wait_returns_when_torrent_job_is_cancelled() {
        let (state, _, album_rel) = seed_torrent_cue_album().await;
        let job_id = download_jobs::insert_queued(
            &state.data.sqlx_pool(),
            DownloadJobType::Torrent,
            0,
            0,
            None,
        )
        .await
        .unwrap();
        assert!(
            download_jobs::claim_running(&state.data.sqlx_pool(), job_id)
                .await
                .unwrap()
        );
        let album_id = albums::id_by_path(&state.data.sqlx_pool(), &album_rel)
            .await
            .unwrap()
            .unwrap();
        let convert_job_id = convert_jobs::create(
            &state.data.sqlx_pool(),
            album_id,
            convert_jobs::ConvertTrigger::Manual,
            1,
        )
        .await
        .unwrap();

        download_jobs::cancel(&state.data.sqlx_pool(), job_id)
            .await
            .unwrap();
        let stopped = wait_convert_finished_or_stopped(&state.data, job_id, convert_job_id)
            .await
            .unwrap();

        assert!(stopped);
    }

    #[tokio::test]
    async fn conversion_wait_errors_when_convert_job_disappears() {
        let state = crate::app::test_support::test_state_without_worker().await;
        let job_id = download_jobs::insert_queued(
            &state.data.sqlx_pool(),
            DownloadJobType::Torrent,
            0,
            0,
            None,
        )
        .await
        .unwrap();
        assert!(
            download_jobs::claim_running(&state.data.sqlx_pool(), job_id)
                .await
                .unwrap()
        );

        let error = wait_convert_finished_or_stopped(&state.data, job_id, 999)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("convert job 999 not found"));
    }

    #[tokio::test]
    async fn split_after_download_runs_storage_split_instead_of_worker_placeholder() {
        let (state, album_dir, album_rel) = seed_torrent_cue_album().await;
        let job_id = download_jobs::insert_queued(
            &state.data.sqlx_pool(),
            DownloadJobType::Torrent,
            0,
            0,
            None,
        )
        .await
        .unwrap();
        assert!(
            download_jobs::claim_running(&state.data.sqlx_pool(), job_id)
                .await
                .unwrap()
        );
        let deps = worker_deps_from_state(&state);
        let post = TorrentPostDownloadOptions {
            split_after_download: true,
            cue_path: Some("album.cue".into()),
            source_file_policy: Some("keep".into()),
            ..Default::default()
        };

        let result = run_torrent_post_download(job_id, &deps, &album_rel, &post).await;

        result.unwrap();
        assert!(album_dir.join("01 - Torrent Artist - One.flac").exists());
        assert!(album_dir.join("02 - Torrent Artist - Two.flac").exists());
    }

    #[test]
    fn torrent_split_cue_path_validation_rejects_missing_escaping_and_non_cue_paths() {
        let mut post = TorrentPostDownloadOptions {
            split_after_download: true,
            ..Default::default()
        };

        let missing = torrent_post_cue_path("Artist/Album", &post).unwrap_err();
        assert!(missing.to_string().contains("cue_path is required"));

        post.cue_path = Some("../album.cue".into());
        let escaping = torrent_post_cue_path("Artist/Album", &post).unwrap_err();
        assert!(escaping.to_string().contains("must not escape root"));

        post.cue_path = Some("album.flac".into());
        let non_cue = torrent_post_cue_path("Artist/Album", &post).unwrap_err();
        assert!(non_cue.to_string().contains("must point to a .cue file"));
    }

    #[tokio::test]
    async fn split_after_conversion_without_conversion_fails_with_clear_prerequisite() {
        let state = crate::app::test_support::test_state_without_worker().await;
        let job_id = download_jobs::insert_queued(
            &state.data.sqlx_pool(),
            DownloadJobType::Torrent,
            0,
            0,
            None,
        )
        .await
        .unwrap();
        assert!(
            download_jobs::claim_running(&state.data.sqlx_pool(), job_id)
                .await
                .unwrap()
        );
        let deps = worker_deps_from_state(&state);
        let post = TorrentPostDownloadOptions {
            split_after_conversion: true,
            cue_path: Some("album.cue".into()),
            ..Default::default()
        };

        let error = run_torrent_post_download(job_id, &deps, "Artist/Album", &post)
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("split_after_conversion requires convert_after_download")
        );
    }

    #[tokio::test]
    async fn split_after_conversion_waits_for_conversion_success_then_runs_storage_split() {
        let (state, album_dir, album_rel) = seed_torrent_cue_album().await;
        let job_id = download_jobs::insert_queued(
            &state.data.sqlx_pool(),
            DownloadJobType::Torrent,
            0,
            0,
            None,
        )
        .await
        .unwrap();
        assert!(
            download_jobs::claim_running(&state.data.sqlx_pool(), job_id)
                .await
                .unwrap()
        );
        let album_id = albums::id_by_path(&state.data.sqlx_pool(), &album_rel)
            .await
            .unwrap()
            .unwrap();
        let pool = state.data.sqlx_pool();
        let finisher = tokio::spawn(async move {
            loop {
                if let Some(row) = convert_jobs::latest_for_album(&pool, album_id)
                    .await
                    .unwrap()
                {
                    convert_jobs::finish(
                        &pool,
                        row.id,
                        convert_jobs::ConvertJobStatus::Success,
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        let deps = worker_deps_from_state(&state);
        let post = TorrentPostDownloadOptions {
            convert_after_download: true,
            split_after_conversion: true,
            cue_path: Some("album.cue".into()),
            source_file_policy: Some("keep".into()),
            ..Default::default()
        };

        run_torrent_post_download(job_id, &deps, &album_rel, &post)
            .await
            .unwrap();
        finisher.await.unwrap();

        assert!(album_dir.join("01 - Torrent Artist - One.flac").exists());
        assert!(album_dir.join("02 - Torrent Artist - Two.flac").exists());
    }

    #[tokio::test]
    async fn cancellation_before_torrent_cue_split_prevents_output_writes() {
        let (state, album_dir, album_rel) = seed_torrent_cue_album().await;
        let job_id = download_jobs::insert_queued(
            &state.data.sqlx_pool(),
            DownloadJobType::Torrent,
            0,
            0,
            None,
        )
        .await
        .unwrap();
        assert!(
            download_jobs::claim_running(&state.data.sqlx_pool(), job_id)
                .await
                .unwrap()
        );
        download_jobs::cancel(&state.data.sqlx_pool(), job_id)
            .await
            .unwrap();
        let deps = worker_deps_from_state(&state);
        let post = TorrentPostDownloadOptions {
            split_after_download: true,
            cue_path: Some("album.cue".into()),
            source_file_policy: Some("keep".into()),
            ..Default::default()
        };

        run_torrent_post_download(job_id, &deps, &album_rel, &post)
            .await
            .unwrap();

        assert!(!album_dir.join("01 - Torrent Artist - One.flac").exists());
        assert!(!album_dir.join("02 - Torrent Artist - Two.flac").exists());
    }
}

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use euterpe_converter::{ConvertOptions, ConvertProgress, FilePolicy, FlacEncodeSettings};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, broadcast, mpsc};
use tokio::task::JoinSet;

use crate::api::{ConvertFileProgress, ConvertProgressEvent};
use crate::config::AppConfig;
use crate::db::convert_jobs::{self, ConvertFileStatus, ConvertJobStatus};
use crate::db::tracks;
use crate::error::ApiError;
use crate::library::storage::{self, LibraryStorage, StoragePath};
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

fn mark_status_failed(
    statuses: &Arc<Mutex<Vec<ConvertFileStatus>>>,
    idx: usize,
    error: String,
) -> Vec<ConvertFileStatus> {
    let mut s = statuses.lock().expect("convert statuses lock");
    if let Some(slot) = s.get_mut(idx) {
        slot.status = "failed".into();
        slot.error = Some(error);
        slot.progress_pct = None;
    }
    s.clone()
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
    let payload = serde_json::to_string(snapshot).map_err(|e| ApiError::Message(e.to_string()))?;
    let done = files_completed_count(snapshot);
    let total = snapshot.len() as i64;
    let pct = job_progress_pct(snapshot);
    let updated =
        convert_jobs::update_progress(&deps.pool, job_id, done, total, pct, Some(&payload)).await?;
    if updated {
        emit_convert_progress(deps, job_id, album_id, "running", snapshot, None);
    }
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
                    tracing::error!(
                        job_id = id,
                        "could not mark convert job failed: {finish_err}"
                    );
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
    let storage = library_storage_from_deps(deps).await?;
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
        emit_convert_progress(deps, job_id, row.album_id, "success", &[], None);
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
    let _ = convert_jobs::update_progress(
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
        let permit = sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| ApiError::Message(format!("convert semaphore: {e}")))?;
        let flac = flac.clone();
        let storage = storage.clone();
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
            let src_path = match StoragePath::parse(&path_rel) {
                Ok(path) => path,
                Err(e) => return (idx, track_id, path_rel, Err(e)),
            };
            let input_bytes = match storage.read(&src_path).await {
                Ok(bytes) => bytes,
                Err(e) => return (idx, track_id, path_rel, Err(e)),
            };
            let statuses_cb = Arc::clone(&statuses);
            let deps_cb = Arc::clone(&deps_spawn);
            let throttle = Arc::new(EncodeProgressThrottle::new());
            let handle = tokio::runtime::Handle::current();
            let progress: Arc<dyn Fn(ConvertProgress) + Send + Sync> =
                Arc::new(move |p: ConvertProgress| {
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
                });
            let input_rel = std::path::PathBuf::from(&path_rel);
            let convert_result = tokio::task::spawn_blocking(move || {
                euterpe_converter::convert_bytes(
                    euterpe_converter::ConvertInput {
                        rel_path: input_rel,
                        bytes: input_bytes.to_vec(),
                    },
                    ConvertOptions {
                        flac_encode: &flac,
                        file_policy,
                        on_progress: Some(progress),
                    },
                )
            })
            .await
            .map_err(|e| ApiError::Message(e.to_string()))
            .and_then(|result| result.map_err(|e| ApiError::Message(e.to_string())));
            (idx, track_id, path_rel, convert_result)
        });
    }

    while let Some(res) = join_set.join_next().await {
        match res {
            Ok((idx, track_id, old_rel, task_result)) => match task_result {
                Ok(converted) => {
                    let new_rel = storage_rel_path_to_string(&converted.rel_path);
                    let output_path = match StoragePath::parse(&new_rel) {
                        Ok(path) => path,
                        Err(e) => {
                            tracing::error!(job_id, path = %old_rel, error = %e, "convert output path invalid");
                            let snapshot = mark_status_failed(&statuses, idx, e.to_string());
                            persist_convert_snapshot(deps, job_id, row.album_id, &snapshot).await?;
                            continue;
                        }
                    };
                    if let Some(parent) = output_path.parent()
                        && let Err(e) = storage.create_dir_all(&parent).await
                    {
                        tracing::error!(job_id, path = %old_rel, error = %e, "convert output directory create failed");
                        let snapshot = mark_status_failed(&statuses, idx, e.to_string());
                        persist_convert_snapshot(deps, job_id, row.album_id, &snapshot).await?;
                        continue;
                    }
                    let bytes_len = converted.bytes.len();
                    let file_hash = hash_bytes(&converted.bytes);
                    if let Err(e) = storage
                        .atomic_write(&output_path, Bytes::from(converted.bytes))
                        .await
                    {
                        tracing::error!(job_id, path = %old_rel, error = %e, "convert output write failed");
                        let snapshot = mark_status_failed(&statuses, idx, e.to_string());
                        persist_convert_snapshot(deps, job_id, row.album_id, &snapshot).await?;
                        continue;
                    }
                    if let Some(delete_rel) = converted.source_delete_rel {
                        let delete_rel = storage_rel_path_to_string(&delete_rel);
                        match StoragePath::parse(&delete_rel) {
                            Ok(delete_path) if delete_path != output_path => {
                                if let Err(e) = storage.delete(&delete_path).await {
                                    tracing::warn!(
                                        job_id,
                                        path = %delete_rel,
                                        error = %e,
                                        "convert source delete failed"
                                    );
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(
                                    job_id,
                                    path = %delete_rel,
                                    error = %e,
                                    "convert source delete path invalid"
                                );
                            }
                        }
                    }
                    match tracks::update_path_fingerprint(
                        &deps.pool,
                        track_id,
                        &new_rel,
                        i64::try_from(bytes_len).ok(),
                        Some(&file_hash),
                        None,
                    )
                    .await
                    {
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
                Err(e) => {
                    tracing::error!(job_id, path = %old_rel, error = %e, "convert failed");
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
    let payload = serde_json::to_string(&snapshot).map_err(|e| ApiError::Message(e.to_string()))?;
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
        && let Ok(scan_root) = StoragePath::parse(album_path)
    {
        let scan_cfg = deps
            .runtime
            .read()
            .await
            .library_scan_config(deps.config.debug)?;
        let _ = crate::services::library_scan::start_scan_storage(
            &deps.pool,
            storage,
            deps.scan_events.clone(),
            scan_cfg,
            Some(scan_root),
            Some(deps.job_tx.clone()),
            Some(deps.runtime.clone()),
        )
        .await;
    }

    Ok(())
}

async fn library_storage_from_deps(
    deps: &ConvertWorkerDeps,
) -> Result<Arc<dyn LibraryStorage>, ApiError> {
    let storage = deps.runtime.read().await.storage.library.clone();
    let location = storage.ok_or_else(|| {
        ApiError::Message(
            "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
        )
    })?;
    storage::storage_from_location(&location, deps.config.master_key.as_ref())
}

fn storage_rel_path_to_string(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
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
        return Err(ApiError::bad_request(
            "convert job already running for album",
        ));
    }
    let track_rows = tracks::list_by_album(pool, album_id).await?;
    let convertible = track_rows
        .iter()
        .filter(|t| is_convertible_path(Path::new(&t.path)))
        .count() as i64;
    if convertible == 0 {
        return Err(ApiError::bad_request("no convertible tracks in album"));
    }
    let id = convert_jobs::create(
        pool,
        album_id,
        convert_jobs::ConvertTrigger::Manual,
        convertible,
    )
    .await?;
    wake(job_tx);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, LibraryScanConfig};
    use crate::db::{albums, artists, connect, migrate};
    use crate::services::app_settings::{ConverterSettings, RuntimeSettings, StorageSettings};
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    fn test_config(library_path: std::path::PathBuf) -> AppConfig {
        AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: None,
            public_base_url: "http://127.0.0.1:0".into(),
            oauth_state_ttl: Duration::from_secs(600),
            qobuz_api_base: None,
            qobuz_play_base: None,
            library_path,
            torrent_incoming_dir: None,
            torrent_max_active: 1,
            torrent_enable_upnp: false,
            download_concurrency: 1,
            library_scan: LibraryScanConfig {
                worker_total: 2,
                enum_workers: 1,
                process_workers: 1,
                seed_depth: 1,
                index_queue_capacity: 16,
                path_queue_capacity: 16,
                debug: false,
            },
            debug: false,
            static_dir: std::path::PathBuf::new(),
        }
    }

    fn write_test_wav(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..512 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[tokio::test]
    async fn execute_job_converts_with_settings_storage_not_config_library_path() {
        let storage_dir = TempDir::new().unwrap();
        let config_dir = TempDir::new().unwrap();
        let rel = "Artist/Album/01.wav";
        write_test_wav(&storage_dir.path().join(rel));

        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let artist_id = artists::upsert_by_name(&pool, "Artist", None)
            .await
            .unwrap();
        let album_id = albums::upsert(
            &pool,
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
        .unwrap();
        let track_id = tracks::upsert(
            &pool,
            tracks::TrackUpsert {
                album_id,
                title: "Track",
                track_number: Some(1),
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: rel,
                duration_sec: None,
                file_mtime: Some("old"),
                file_hash: Some("old"),
                file_size: Some(1),
            },
        )
        .await
        .unwrap();
        let job_id = convert_jobs::create(&pool, album_id, convert_jobs::ConvertTrigger::Manual, 1)
            .await
            .unwrap();

        let runtime = RuntimeSettings {
            storage: StorageSettings::local(storage_dir.path().display().to_string()),
            converter: ConverterSettings { parallelism: 1, ..Default::default() },
            ..Default::default()
        };
        let runtime = Arc::new(RwLock::new(runtime));
        let (events, _) = broadcast::channel(8);
        let (scan_events, _) = broadcast::channel(8);
        let (job_tx, _job_rx) = mpsc::channel(8);
        let deps = Arc::new(ConvertWorkerDeps {
            pool: pool.clone(),
            config: Arc::new(test_config(config_dir.path().to_path_buf())),
            runtime,
            events,
            scan_events,
            job_tx,
        });

        execute_job(job_id, &deps).await.unwrap();

        assert!(!storage_dir.path().join(rel).exists());
        let out = storage_dir.path().join("Artist/Album/01.flac");
        assert!(out.exists());
        assert!(!config_dir.path().join("Artist/Album/01.flac").exists());

        let track = tracks::get_by_id(&pool, track_id).await.unwrap().unwrap();
        assert_eq!(track.path, "Artist/Album/01.flac");
        assert_eq!(
            track.file_size,
            Some(std::fs::metadata(&out).unwrap().len() as i64)
        );
        assert!(track.file_hash.is_some());
        assert_ne!(track.file_hash.as_deref(), Some("old"));
        assert_eq!(track.file_mtime, None);

        let job = convert_jobs::get_by_id(&pool, job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.status, "success");
    }
}

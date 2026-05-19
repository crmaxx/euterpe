use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use flume::Sender;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc};

use crate::api::ScanProgressEvent;
use crate::config::LibraryScanConfig;
use crate::db::{albums, artists, library_scan_runs, tracks};
use crate::error::ApiError;
use crate::library::covers::discover_album_cover_rel;
use crate::library::fs::file_stat_sync;
use crate::library::paths::resolve_scan_subdirectory;
use crate::library::tags::{self, is_audio_file, TrackTags};

const PROGRESS_EVERY: usize = 5;

macro_rules! scan_debug {
    ($debug:expr, $($arg:tt)*) => {
        if $debug {
            tracing::debug!($($arg)*);
        }
    };
}

#[derive(Clone)]
pub struct ScanDeps {
    pub pool: SqlitePool,
    pub library_path: PathBuf,
    pub events: broadcast::Sender<ScanProgressEvent>,
    pub scan: LibraryScanConfig,
    /// Subtree only (canonical absolute path under `library_path`); `None` = full library.
    pub scan_root: Option<PathBuf>,
}

#[derive(Eq, PartialEq)]
struct DirTask {
    path: PathBuf,
    depth: u32,
}

impl Ord for DirTask {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.depth.cmp(&other.depth)
    }
}

impl PartialOrd for DirTask {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

struct DirWorkQueue {
    heap: BinaryHeap<DirTask>,
    visited: HashSet<PathBuf>,
    /// Workers currently inside `enumerate_dir_level` (avoids early exit while re-enqueueing).
    active_workers: usize,
}

impl DirWorkQueue {
    fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            visited: HashSet::new(),
            active_workers: 0,
        }
    }

    fn try_enqueue(&mut self, path: PathBuf, depth: u32) -> bool {
        let canon = path.canonicalize().unwrap_or(path);
        if !self.visited.insert(canon.clone()) {
            return false;
        }
        self.heap.push(DirTask { path: canon, depth });
        true
    }

    fn pop(&mut self) -> Option<DirTask> {
        self.heap.pop()
    }

    fn should_shutdown(&self) -> bool {
        self.heap.is_empty() && self.active_workers == 0
    }
}

#[derive(Clone)]
struct ScanProgressCounters {
    files_seen: Arc<AtomicI64>,
    files_processed: Arc<AtomicI64>,
    files_indexed: Arc<AtomicI64>,
    files_total_final: Arc<Mutex<Option<i64>>>,
    events: broadcast::Sender<ScanProgressEvent>,
}

struct ProcessWorkerChannels {
    path_rx: flume::Receiver<PathBuf>,
    index_tx: mpsc::Sender<ScanIndexJob>,
}

/// Per-worker scan state for directory enumeration (`clippy::too_many_arguments` ≤ 7).
struct EnumerateContext<'a> {
    scan_id: i64,
    worker_id: usize,
    pool: &'a SqlitePool,
    dir_queue: &'a Arc<Mutex<DirWorkQueue>>,
    path_tx: &'a Sender<PathBuf>,
    counters: &'a ScanProgressCounters,
    debug: bool,
}

/// Ready-to-persist index payload (no further disk I/O on the DB writer).
struct ScanIndexJob {
    path_rel: String,
    album_path_rel: String,
    tags: TrackTags,
    file_mtime: Option<String>,
    file_hash: Option<String>,
    file_size: Option<i64>,
    cover_path: Option<String>,
}

pub async fn run_scan(scan_id: i64, deps: ScanDeps) {
    if let Err(e) = run_scan_inner(scan_id, &deps).await {
        if library_scan_runs::is_cancelled(&deps.pool, scan_id)
            .await
            .unwrap_or(false)
        {
            return;
        }
        tracing::error!(scan_id, error = %e, "library scan failed");
        let _ = library_scan_runs::finish_failed(&deps.pool, scan_id, &e.to_string()).await;
    }
}

pub async fn request_cancel(pool: &SqlitePool, scan_id: i64) -> Result<(), ApiError> {
    if library_scan_runs::cancel(pool, scan_id).await? {
        return Ok(());
    }
    let run = library_scan_runs::get_by_id(pool, scan_id).await?;
    match run {
        None => Err(ApiError::Message(format!("scan {scan_id} not found"))),
        Some(r) if r.status != "running" => {
            Err(ApiError::Message("cannot cancel finished scan".into()))
        }
        _ => Err(ApiError::Message(format!("scan {scan_id} not found"))),
    }
}

fn files_total_for_db(files_total_final: &Mutex<Option<i64>>) -> i64 {
    files_total_final.lock().expect("scan files_total lock poisoned").unwrap_or(0)
}

async fn flush_scan_progress(
    scan_id: i64,
    pool: &SqlitePool,
    files_seen: i64,
    files_processed: i64,
    files_indexed: i64,
    files_total_final: &Mutex<Option<i64>>,
    events: &broadcast::Sender<ScanProgressEvent>,
) -> Result<(), ApiError> {
    let total = files_total_for_db(files_total_final);
    library_scan_runs::update_progress(
        pool,
        scan_id,
        files_seen,
        files_processed,
        files_indexed,
        total,
    )
    .await?;
    let _ = events.send(ScanProgressEvent {
        scan_id,
        files_seen,
        files_processed,
        files_indexed,
        files_total: total,
    });
    Ok(())
}

async fn run_scan_inner(scan_id: i64, deps: &ScanDeps) -> Result<(), ApiError> {
    let root = deps
        .library_path
        .canonicalize()
        .map_err(|e| ApiError::Message(format!("canonicalize library path: {e}")))?;
    if !root.is_dir() {
        return Err(ApiError::Message(format!(
            "library path is not a directory: {}",
            root.display()
        )));
    }

    let counters = ScanProgressCounters {
        files_seen: Arc::new(AtomicI64::new(0)),
        files_processed: Arc::new(AtomicI64::new(0)),
        files_indexed: Arc::new(AtomicI64::new(0)),
        files_total_final: Arc::new(Mutex::new(None)),
        events: deps.events.clone(),
    };
    let debug = deps.scan.debug;

    let mut dir_work = DirWorkQueue::new();
    if let Some(sub) = &deps.scan_root {
        let sub = sub
            .canonicalize()
            .map_err(|e| ApiError::Message(format!("canonicalize scan root: {e}")))?;
        scan_debug!(
            debug,
            scan_id,
            dir = %sub.display(),
            "scan subtree root"
        );
        if !dir_work.try_enqueue(sub, 0) {
            return Err(ApiError::Message("scan root already visited".into()));
        }
    } else {
        let seed_dirs = seed_scan_dirs(&root, deps.scan.seed_depth)?;
        for dir in seed_dirs {
            dir_work.try_enqueue(dir, 0);
        }
        scan_debug!(
            debug,
            scan_id,
            seed_dirs = dir_work.heap.len(),
            seed_depth = deps.scan.seed_depth,
            "scan seed directories"
        );
        if dir_work.heap.is_empty() {
            return Err(ApiError::Message(
                "no directories enqueued for library scan".into(),
            ));
        }
    }
    scan_debug!(
        debug,
        scan_id,
        worker_total = deps.scan.worker_total,
        enum_workers = deps.scan.enum_workers,
        process_workers = deps.scan.process_workers,
        path_queue = deps.scan.path_queue_capacity,
        index_queue = deps.scan.index_queue_capacity,
        root = %root.display(),
        "library scan started"
    );
    let dir_queue = Arc::new(Mutex::new(dir_work));

    let (path_tx, path_rx) = flume::bounded::<PathBuf>(deps.scan.path_queue_capacity);
    let (index_tx, index_rx) = mpsc::channel(deps.scan.index_queue_capacity);

    let writer_pool = deps.pool.clone();
    let writer_counters = counters.clone();
    let writer_debug = debug;
    let writer_handle = tokio::spawn(async move {
        run_db_writer(
            scan_id,
            &writer_pool,
            index_rx,
            &writer_counters,
            writer_debug,
        )
        .await
    });

    let n_proc = deps.scan.process_workers;
    let mut proc_handles = Vec::with_capacity(n_proc);
    for worker_id in 0..n_proc {
        let path_rx = path_rx.clone();
        let root = root.clone();
        let pool = deps.pool.clone();
        let index_tx = index_tx.clone();
        let proc_counters = counters.clone();
        let proc_debug = debug;
        proc_handles.push(tokio::spawn(async move {
            process_worker_loop(
                scan_id,
                worker_id,
                &pool,
                &root,
                ProcessWorkerChannels { path_rx, index_tx },
                &proc_counters,
                proc_debug,
            )
            .await
        }));
    }
    drop(path_rx);

    let n_enum = deps.scan.enum_workers;
    let mut enum_handles = Vec::with_capacity(n_enum);
    for worker_id in 0..n_enum {
        let dir_queue = dir_queue.clone();
        let path_tx = path_tx.clone();
        let pool = deps.pool.clone();
        let enum_counters = counters.clone();
        let enum_debug = debug;
        enum_handles.push(tokio::spawn(async move {
            enumerate_worker_loop(
                scan_id,
                worker_id,
                &pool,
                dir_queue,
                path_tx,
                &enum_counters,
                enum_debug,
            )
            .await
        }));
    }

    for handle in enum_handles {
        let _ = handle.await;
    }

    let discovered = counters.files_seen.load(Ordering::Relaxed);
    *counters
        .files_total_final
        .lock()
        .expect("scan files_total lock poisoned") = Some(discovered);
    flush_scan_progress(
        scan_id,
        &deps.pool,
        discovered,
        counters.files_processed.load(Ordering::Relaxed),
        counters.files_indexed.load(Ordering::Relaxed),
        counters.files_total_final.as_ref(),
        &counters.events,
    )
    .await?;
    scan_debug!(
        debug,
        scan_id,
        files_total = discovered,
        "enumerate phase finished"
    );

    drop(path_tx);

    for handle in proc_handles {
        let _ = handle.await;
    }
    drop(index_tx);

    writer_handle
        .await
        .map_err(|e| ApiError::Message(format!("db writer task panicked: {e}")))??;

    let seen = counters.files_seen.load(Ordering::Relaxed);
    let processed = counters.files_processed.load(Ordering::Relaxed);
    let indexed = counters.files_indexed.load(Ordering::Relaxed);
    flush_scan_progress(
        scan_id,
        &deps.pool,
        seen,
        processed,
        indexed,
        counters.files_total_final.as_ref(),
        &counters.events,
    )
    .await?;

    if library_scan_runs::is_cancelled(&deps.pool, scan_id).await? {
        scan_debug!(debug, scan_id, "library scan cancelled");
        return Ok(());
    }

    library_scan_runs::finish_success(&deps.pool, scan_id).await?;
    scan_debug!(
        debug,
        scan_id,
        files_seen = seen,
        files_processed = processed,
        files_indexed = indexed,
        files_total = files_total_for_db(counters.files_total_final.as_ref()),
        "library scan finished"
    );
    Ok(())
}

async fn enumerate_worker_loop(
    scan_id: i64,
    worker_id: usize,
    pool: &SqlitePool,
    dir_queue: Arc<Mutex<DirWorkQueue>>,
    path_tx: Sender<PathBuf>,
    counters: &ScanProgressCounters,
    debug: bool,
) -> Result<(), ApiError> {
    scan_debug!(debug, scan_id, worker_id, "enumerate worker started");
    loop {
        let task = {
            let mut q = dir_queue.lock().expect("scan dir queue poisoned");
            if q.should_shutdown() {
                scan_debug!(
                    debug,
                    scan_id,
                    worker_id,
                    "enumerate worker finished (queue empty)"
                );
                return Ok(());
            }
            if let Some(task) = q.pop() {
                q.active_workers += 1;
                Some(task)
            } else {
                None
            }
        };
        let Some(task) = task else {
            tokio::task::yield_now().await;
            continue;
        };
        scan_debug!(
            debug,
            scan_id,
            worker_id,
            dir = %task.path.display(),
            depth = task.depth,
            "enumerate worker claimed directory"
        );
        let ctx = EnumerateContext {
            scan_id,
            worker_id,
            pool,
            dir_queue: &dir_queue,
            path_tx: &path_tx,
            counters,
            debug,
        };
        let result = enumerate_dir_level(&ctx, &task.path, task.depth).await;
        {
            let mut q = dir_queue.lock().expect("scan dir queue poisoned");
            q.active_workers = q.active_workers.saturating_sub(1);
        }
        match result {
            Ok(()) => scan_debug!(
                debug,
                scan_id,
                worker_id,
                dir = %task.path.display(),
                "enumerate directory done"
            ),
            Err(e) => tracing::warn!(
                scan_id,
                worker_id,
                dir = %task.path.display(),
                error = %e,
                "enumerate directory failed"
            ),
        }
    }
}

async fn enumerate_dir_level(
    ctx: &EnumerateContext<'_>,
    dir: &Path,
    depth: u32,
) -> Result<(), ApiError> {
    let entries = std::fs::read_dir(dir).map_err(|e| ApiError::Message(e.to_string()))?;
    for entry in entries {
        if library_scan_runs::is_cancelled(ctx.pool, ctx.scan_id).await? {
            return Ok(());
        }
        let entry = entry.map_err(|e| ApiError::Message(e.to_string()))?;
        let path = entry.path();
        let ft = entry
            .file_type()
            .map_err(|e| ApiError::Message(e.to_string()))?;
        if ft.is_dir() {
            let sub = path.clone();
            let enqueued = {
                let mut q = ctx.dir_queue.lock().expect("scan dir queue poisoned");
                q.try_enqueue(sub.clone(), depth.saturating_add(1))
            };
            if enqueued {
                scan_debug!(
                    ctx.debug,
                    ctx.scan_id,
                    ctx.worker_id,
                    dir = %sub.display(),
                    "enumerate enqueued subdirectory"
                );
            }
            continue;
        }
        if !ft.is_file() || !is_audio_file(&path) {
            continue;
        }

        ctx.path_tx
            .send_async(path)
            .await
            .map_err(|_| ApiError::Message("path queue closed".into()))?;

        let seen = ctx.counters.files_seen.fetch_add(1, Ordering::Relaxed) + 1;
        if (seen as usize).is_multiple_of(PROGRESS_EVERY) {
            flush_scan_progress(
                ctx.scan_id,
                ctx.pool,
                seen,
                ctx.counters.files_processed.load(Ordering::Relaxed),
                ctx.counters.files_indexed.load(Ordering::Relaxed),
                ctx.counters.files_total_final.as_ref(),
                &ctx.counters.events,
            )
            .await?;
            scan_debug!(
                ctx.debug,
                ctx.scan_id,
                ctx.worker_id,
                files_seen = seen,
                "enumerate progress"
            );
        }
    }
    Ok(())
}

async fn process_worker_loop(
    scan_id: i64,
    worker_id: usize,
    pool: &SqlitePool,
    root: &Path,
    channels: ProcessWorkerChannels,
    counters: &ScanProgressCounters,
    debug: bool,
) -> Result<(), ApiError> {
    let ProcessWorkerChannels { path_rx, index_tx } = channels;
    scan_debug!(debug, scan_id, worker_id, "process worker started");
    while let Ok(abs_path) = path_rx.recv_async().await {
        if library_scan_runs::is_cancelled(pool, scan_id).await? {
            break;
        }

        let path_rel = match abs_path.strip_prefix(root) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => {
                tracing::warn!(path = %abs_path.display(), "path outside library root");
                continue;
            }
        };

        let abs_for_stat = abs_path.clone();
        let (mtime, size) = tokio::task::spawn_blocking(move || file_stat_sync(&abs_for_stat))
            .await
            .map_err(|e| ApiError::Message(format!("stat task join: {e}")))?;

        if let Some((db_mtime, db_size)) =
            tracks::get_fingerprint_by_path(pool, &path_rel).await?
        {
            let size_i64 = i64::try_from(size).ok();
            if db_mtime.as_deref() == mtime.as_deref()
                && db_size.is_some()
                && db_size == size_i64
            {
                scan_debug!(
                    debug,
                    scan_id,
                    worker_id,
                    path = %path_rel,
                    "skip unchanged file"
                );
                let processed = counters.files_processed.fetch_add(1, Ordering::Relaxed) + 1;
                let indexed = counters.files_indexed.fetch_add(1, Ordering::Relaxed) + 1;
                if (processed as usize).is_multiple_of(PROGRESS_EVERY) {
                    flush_scan_progress(
                        scan_id,
                        pool,
                        counters.files_seen.load(Ordering::Relaxed),
                        processed,
                        indexed,
                        counters.files_total_final.as_ref(),
                        &counters.events,
                    )
                    .await?;
                }
                continue;
            }
        }

        let job = match collect_index_job(root, &abs_path, mtime, size).await {
            Ok(job) => job,
            Err(e) => {
                tracing::warn!(path = %abs_path.display(), error = %e, "skip file during scan");
                continue;
            }
        };
        scan_debug!(
            debug,
            scan_id,
            worker_id,
            path = %job.path_rel,
            album = %job.album_path_rel,
            "queued index job"
        );
        index_tx
            .send(job)
            .await
            .map_err(|_| ApiError::Message("index queue closed".into()))?;

        let processed = counters.files_processed.fetch_add(1, Ordering::Relaxed) + 1;
        if (processed as usize).is_multiple_of(PROGRESS_EVERY) {
            flush_scan_progress(
                scan_id,
                pool,
                counters.files_seen.load(Ordering::Relaxed),
                processed,
                counters.files_indexed.load(Ordering::Relaxed),
                counters.files_total_final.as_ref(),
                &counters.events,
            )
            .await?;
            scan_debug!(
                debug,
                scan_id,
                worker_id,
                files_processed = processed,
                "process progress"
            );
        }
    }
    scan_debug!(debug, scan_id, worker_id, "process worker finished");
    Ok(())
}

fn seed_scan_dirs(root: &Path, seed_depth: u32) -> Result<Vec<PathBuf>, ApiError> {
    if seed_depth == 0 {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut frontier = vec![root.to_path_buf()];
    for _ in 0..seed_depth {
        let mut next = Vec::new();
        for dir in frontier {
            for entry in std::fs::read_dir(&dir).map_err(|e| ApiError::Message(e.to_string()))? {
                let entry = entry.map_err(|e| ApiError::Message(e.to_string()))?;
                let ft = entry
                    .file_type()
                    .map_err(|e| ApiError::Message(e.to_string()))?;
                if ft.is_dir() {
                    next.push(entry.path());
                }
            }
        }
        if next.is_empty() {
            return Ok(vec![root.to_path_buf()]);
        }
        frontier = next;
    }
    Ok(frontier)
}

async fn collect_index_job(
    root: &Path,
    path: &Path,
    file_mtime: Option<String>,
    size_bytes: u64,
) -> Result<ScanIndexJob, ApiError> {
    let track_tags = tags::read_tags(path)?;
    let rel = path
        .strip_prefix(root)
        .map_err(|_| ApiError::Message("path outside library root".into()))?;
    let path_rel = rel.to_string_lossy().replace('\\', "/");
    let album_dir = path
        .parent()
        .ok_or_else(|| ApiError::Message("no parent".into()))?;
    let album_path_rel = album_dir
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| album_dir.to_string_lossy().into_owned());
    let cover_path = discover_album_cover_rel(root, &album_path_rel);

    let path_owned = path.to_path_buf();
    let file_hash = tokio::task::spawn_blocking(move || file_hash_sync(&path_owned))
        .await
        .map_err(|e| ApiError::Message(format!("hash task join: {e}")))??;

    let file_size = i64::try_from(size_bytes).ok();

    Ok(ScanIndexJob {
        path_rel,
        album_path_rel,
        tags: track_tags,
        file_mtime,
        file_hash,
        file_size,
        cover_path,
    })
}

fn file_hash_sync(path: &Path) -> Result<Option<String>, ApiError> {
    let mut hasher = Sha256::new();
    let bytes = std::fs::read(path).map_err(|e| ApiError::Message(e.to_string()))?;
    hasher.update(bytes);
    Ok(Some(hex::encode(hasher.finalize())))
}

async fn run_db_writer(
    scan_id: i64,
    pool: &SqlitePool,
    mut index_rx: mpsc::Receiver<ScanIndexJob>,
    counters: &ScanProgressCounters,
    debug: bool,
) -> Result<(), ApiError> {
    scan_debug!(debug, scan_id, "db writer started");
    while let Some(job) = index_rx.recv().await {
        if library_scan_runs::is_cancelled(pool, scan_id).await? {
            break;
        }
        let path_rel = job.path_rel.clone();
        match persist_index(pool, job).await {
            Ok(()) => {
                let indexed = counters.files_indexed.fetch_add(1, Ordering::Relaxed) + 1;
                scan_debug!(
                    debug,
                    scan_id,
                    path = %path_rel,
                    files_indexed = indexed,
                    "persisted track"
                );
                if (indexed as usize).is_multiple_of(PROGRESS_EVERY) {
                    flush_scan_progress(
                        scan_id,
                        pool,
                        counters.files_seen.load(Ordering::Relaxed),
                        counters.files_processed.load(Ordering::Relaxed),
                        indexed,
                        counters.files_total_final.as_ref(),
                        &counters.events,
                    )
                    .await?;
                    scan_debug!(
                        debug,
                        scan_id,
                        files_indexed = indexed,
                        "db writer progress"
                    );
                }
            }
            Err(e) => tracing::warn!(path = %path_rel, error = %e, "skip file during scan"),
        }
    }
    scan_debug!(debug, scan_id, "db writer finished");
    Ok(())
}

async fn persist_index(pool: &SqlitePool, job: ScanIndexJob) -> Result<(), ApiError> {
    let tags = &job.tags;
    let artist_id = artists::upsert_by_name(pool, &tags.artist, None).await?;
    let year = tags.year.map(|y| y as i32);
    let album_id = albums::upsert(
        pool,
        albums::AlbumUpsert {
            artist_id: Some(artist_id),
            title: &tags.album,
            year,
            qobuz_album_id: tags.qobuz_album_id.map(|id| id as i64),
            path: Some(&job.album_path_rel),
            cover_path: job.cover_path.as_deref(),
        },
    )
    .await?;

    tracks::upsert(
        pool,
        tracks::TrackUpsert {
            album_id,
            title: &tags.title,
            track_number: tags.track_number.map(|n| n as i32),
            year: tags.year.map(|y| y as i32),
            disc_number: tags.disc_number.map(|d| d as i32),
            genre: tags
                .genre
                .as_deref()
                .and_then(|g| if g.is_empty() { None } else { Some(g) }),
            qobuz_track_id: tags.qobuz_track_id.map(|id| id as i64),
            path: &job.path_rel,
            duration_sec: tags.duration_sec.map(|d| d as i32),
            file_mtime: job.file_mtime.as_deref(),
            file_hash: job.file_hash.as_deref(),
            file_size: job.file_size,
        },
    )
    .await?;
    Ok(())
}

pub fn spawn_scan(scan_id: i64, deps: ScanDeps) {
    tokio::spawn(async move {
        run_scan(scan_id, deps).await;
    });
}

pub async fn start_scan(
    pool: &SqlitePool,
    library_path: PathBuf,
    events: broadcast::Sender<ScanProgressEvent>,
    scan: LibraryScanConfig,
    scan_root: Option<PathBuf>,
) -> Result<i64, ApiError> {
    if library_scan_runs::has_running(pool).await? {
        return Err(ApiError::Message("SCAN_ALREADY_RUNNING".into()));
    }
    let scan_id = library_scan_runs::start(pool).await?;
    spawn_scan(
        scan_id,
        ScanDeps {
            pool: pool.clone(),
            library_path,
            events,
            scan,
            scan_root,
        },
    );
    Ok(scan_id)
}

pub fn resolve_scan_root_query(
    library_path: &Path,
    root: Option<&str>,
) -> Result<Option<PathBuf>, ApiError> {
    match root {
        None => Ok(None),
        Some(r) => Ok(Some(resolve_scan_subdirectory(library_path, r)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LibraryScanConfig;
    use crate::db::{connect, migrate};
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    fn write_test_wav_with_tags(
        album_dir: &Path,
        filename: &str,
        tags: tags::TrackTags,
    ) -> PathBuf {
        std::fs::create_dir_all(album_dir).unwrap();
        let track_path = album_dir.join(filename);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&track_path, spec).unwrap();
        for _ in 0..512 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
        tags::write_tags(&track_path, &tags).unwrap();
        track_path
    }

    fn scan_cfg_1_1() -> LibraryScanConfig {
        LibraryScanConfig {
            worker_total: 2,
            enum_workers: 1,
            process_workers: 1,
            path_queue_capacity: 64,
            index_queue_capacity: 64,
            ..LibraryScanConfig::default()
        }
    }

    #[tokio::test]
    async fn scan_indexes_audio_files() {
        let dir = TempDir::new().unwrap();
        let artist_dir = dir.path().join("Artist A").join("Album One");
        write_test_wav_with_tags(
            &artist_dir,
            "01.wav",
            tags::TrackTags {
                title: "Song".into(),
                artist: "Artist A".into(),
                album: "Album One".into(),
                track_number: Some(1),
                year: Some(2020),
                disc_number: None,
                genre: None,
                duration_sec: None,
                qobuz_track_id: None,
                qobuz_album_id: None,
                label: None,
                isrc: None,
                composer: None,
            },
        );

        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let (events, _) = broadcast::channel(8);
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_scan(
            scan_id,
            ScanDeps {
                pool: pool.clone(),
                library_path: dir.path().to_path_buf(),
                events,
                scan: scan_cfg_1_1(),
                scan_root: None,
            },
        )
        .await;

        let run = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.status, "success", "run: {run:?}");
        assert_eq!(run.files_seen, 1, "run: {run:?}");
        assert_eq!(run.files_total, 1, "run: {run:?}");
        assert_eq!(run.files_processed, 1, "run: {run:?}");
        assert_eq!(run.files_indexed, 1, "run: {run:?}");

        use crate::api::SortOrder;
        use crate::db::albums::{AlbumsListParams, AlbumsSort};
        let page = albums::list_keyset(
            &pool,
            AlbumsListParams {
                sort: AlbumsSort::Title,
                order: SortOrder::Asc,
                limit: 10,
                q: None,
                cursor: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].title, "Album One");
    }

    #[tokio::test]
    async fn parallel_scan_indexes_multiple_artists_without_duplicate_paths() {
        let dir = TempDir::new().unwrap();
        write_test_wav_with_tags(
            &dir.path().join("Artist A").join("Album One"),
            "01.wav",
            tags::TrackTags {
                title: "Song A".into(),
                artist: "Artist A".into(),
                album: "Album One".into(),
                track_number: Some(1),
                year: None,
                disc_number: None,
                genre: None,
                duration_sec: None,
                qobuz_track_id: None,
                qobuz_album_id: None,
                label: None,
                isrc: None,
                composer: None,
            },
        );
        write_test_wav_with_tags(
            &dir.path().join("Artist B").join("Album Two"),
            "01.wav",
            tags::TrackTags {
                title: "Song B".into(),
                artist: "Artist B".into(),
                album: "Album Two".into(),
                track_number: Some(1),
                year: None,
                disc_number: None,
                genre: None,
                duration_sec: None,
                qobuz_track_id: None,
                qobuz_album_id: None,
                label: None,
                isrc: None,
                composer: None,
            },
        );

        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let (events, _) = broadcast::channel(8);
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_scan(
            scan_id,
            ScanDeps {
                pool: pool.clone(),
                library_path: dir.path().to_path_buf(),
                events,
                scan: LibraryScanConfig {
                    worker_total: 4,
                    enum_workers: 2,
                    process_workers: 2,
                    seed_depth: 1,
                    index_queue_capacity: 64,
                    path_queue_capacity: 64,
                    debug: false,
                },
                scan_root: None,
            },
        )
        .await;

        let run = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.status, "success");
        assert_eq!(run.files_indexed, 2);
        assert_eq!(run.files_total, 2);
        assert_eq!(run.files_seen, 2);
        assert_eq!(run.files_processed, 2);

        use crate::api::SortOrder;
        use crate::db::albums::{AlbumsListParams, AlbumsSort};
        let page = albums::list_keyset(
            &pool,
            AlbumsListParams {
                sort: AlbumsSort::Title,
                order: SortOrder::Asc,
                limit: 10,
                q: None,
                cursor: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), 2);

        let track_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(track_count.0, 2);
        let distinct_paths: (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT path) FROM tracks")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(distinct_paths.0, 2);
    }

    #[tokio::test]
    async fn scan_skips_unchanged_files_on_rescan() {
        let dir = TempDir::new().unwrap();
        let album_dir = dir.path().join("Artist A").join("Album One");
        write_test_wav_with_tags(
            &album_dir,
            "01.wav",
            tags::TrackTags {
                title: "Song".into(),
                artist: "Artist A".into(),
                album: "Album One".into(),
                track_number: Some(1),
                year: Some(2020),
                disc_number: None,
                genre: None,
                duration_sec: None,
                qobuz_track_id: None,
                qobuz_album_id: None,
                label: None,
                isrc: None,
                composer: None,
            },
        );

        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let (events, _) = broadcast::channel(8);
        let deps = ScanDeps {
            pool: pool.clone(),
            library_path: dir.path().to_path_buf(),
            events: events.clone(),
            scan: scan_cfg_1_1(),
            scan_root: None,
        };

        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_scan(scan_id, deps.clone()).await;
        let first = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.files_indexed, 1);

        let scan_id2 = library_scan_runs::start(&pool).await.unwrap();
        run_scan(scan_id2, deps).await;
        let second = library_scan_runs::get_by_id(&pool, scan_id2)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.status, "success");
        assert_eq!(second.files_indexed, 1);
        assert_eq!(second.files_processed, 1);
    }

    #[tokio::test]
    async fn deep_tree_scan_via_reenqueue() {
        let dir = TempDir::new().unwrap();
        let deep = dir
            .path()
            .join("Artist A")
            .join("Album One")
            .join("nested")
            .join("deep");
        write_test_wav_with_tags(
            &deep,
            "01.wav",
            tags::TrackTags {
                title: "Deep".into(),
                artist: "Artist A".into(),
                album: "Album One".into(),
                track_number: Some(1),
                year: None,
                disc_number: None,
                genre: None,
                duration_sec: None,
                qobuz_track_id: None,
                qobuz_album_id: None,
                label: None,
                isrc: None,
                composer: None,
            },
        );

        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let (events, _) = broadcast::channel(8);
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_scan(
            scan_id,
            ScanDeps {
                pool: pool.clone(),
                library_path: dir.path().to_path_buf(),
                events,
                scan: LibraryScanConfig {
                    worker_total: 4,
                    enum_workers: 2,
                    process_workers: 1,
                    seed_depth: 1,
                    index_queue_capacity: 64,
                    path_queue_capacity: 64,
                    debug: false,
                },
                scan_root: None,
            },
        )
        .await;

        let run = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.status, "success");
        assert_eq!(run.files_indexed, 1);
    }
}

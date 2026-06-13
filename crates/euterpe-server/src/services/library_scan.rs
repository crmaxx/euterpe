use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use flume::Sender;
use futures_util::StreamExt;
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, broadcast, mpsc};

use crate::api::ScanProgressEvent;
use crate::config::LibraryScanConfig;
use crate::db::{albums, artists, library_scan_runs, tracks};
use crate::error::ApiError;
use crate::library::covers::{discover_album_cover_rel, ensure_album_cover_path_storage};
use crate::library::file_hash::ContentXxh64;
use crate::library::fs::file_stat_sync;
use crate::library::paths::resolve_scan_subdirectory;
use crate::library::storage::{LibraryStorage, StorageEntryKind, StoragePath};
use crate::library::tags::{self, TrackTags, is_audio_file};

const PROGRESS_EVERY: usize = 5;
/// Per-directory SMB listing; scan fails clearly instead of hanging forever.
const STORAGE_LIST_DIR_TIMEOUT: Duration = Duration::from_secs(120);
/// Per-file SMB open/read during indexing.
const STORAGE_FILE_OP_TIMEOUT: Duration = Duration::from_secs(120);
/// Per-chunk SMB read while hashing/tagging.
const STORAGE_READ_CHUNK_TIMEOUT: Duration = Duration::from_secs(90);
/// Wall-clock cap per audio file so a stuck NAS read cannot block the whole scan.
const STORAGE_FILE_INDEX_TIMEOUT: Duration = Duration::from_secs(180);
const STORAGE_TAG_HEAD: usize = 2 * 1024 * 1024;
const STORAGE_MAX_READ_BYTES: u64 = 512 * 1024 * 1024;
const STORAGE_HASH_BACKFILL_BATCH: i64 = 256;

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
    pub convert_job_tx: Option<tokio::sync::mpsc::Sender<i64>>,
    pub runtime:
        Option<std::sync::Arc<tokio::sync::RwLock<crate::services::app_settings::RuntimeSettings>>>,
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
    files_total_final
        .lock()
        .expect("scan files_total lock poisoned")
        .unwrap_or(0)
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
    let writer_scan_deps = deps.clone();
    let writer_handle = tokio::spawn(async move {
        run_db_writer(
            scan_id,
            &writer_pool,
            index_rx,
            &writer_counters,
            writer_debug,
            &writer_scan_deps,
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

        if let Some((db_mtime, db_size)) = tracks::get_fingerprint_by_path(pool, &path_rel).await? {
            let size_i64 = i64::try_from(size).ok();
            if db_mtime.as_deref() == mtime.as_deref() && db_size.is_some() && db_size == size_i64 {
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
    let rel = path
        .strip_prefix(root)
        .map_err(|_| ApiError::Message("path outside library root".into()))?;
    let path_rel = rel.to_string_lossy().replace('\\', "/");
    let track_tags = tags::read_tags_with_rel(path, Some(&path_rel))?;
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
    let file = std::fs::File::open(path).map_err(|e| ApiError::Message(e.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = ContentXxh64::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| ApiError::Message(e.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(Some(hasher.finish()))
}

async fn run_db_writer(
    scan_id: i64,
    pool: &SqlitePool,
    mut index_rx: mpsc::Receiver<ScanIndexJob>,
    counters: &ScanProgressCounters,
    debug: bool,
    deps: &ScanDeps,
) -> Result<(), ApiError> {
    scan_debug!(debug, scan_id, "db writer started");
    while let Some(job) = index_rx.recv().await {
        if library_scan_runs::is_cancelled(pool, scan_id).await? {
            break;
        }
        let path_rel = job.path_rel.clone();
        match persist_index(pool, job, deps).await {
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

async fn persist_index(
    pool: &SqlitePool,
    job: ScanIndexJob,
    deps: &ScanDeps,
) -> Result<(), ApiError> {
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

    if let (Some(tx), Some(runtime)) = (&deps.convert_job_tx, &deps.runtime) {
        let auto = runtime.read().await.converter.auto_enabled;
        if auto && tags::is_convertible_path(std::path::Path::new(&job.path_rel)) {
            let convertible = tracks::list_by_album(pool, album_id)
                .await?
                .iter()
                .filter(|t| tags::is_convertible_path(std::path::Path::new(&t.path)))
                .count() as i64;
            if convertible > 0
                && let Some(_id) =
                    crate::db::convert_jobs::enqueue_album_if_needed(pool, album_id, convertible)
                        .await?
            {
                let _ = tx.try_send(0);
            }
        }
    }
    Ok(())
}

pub fn spawn_scan(scan_id: i64, deps: ScanDeps) {
    tokio::spawn(async move {
        run_scan(scan_id, deps).await;
    });
}

#[derive(Clone)]
pub struct StorageScanDeps {
    pub pool: SqlitePool,
    pub storage: Arc<dyn LibraryStorage>,
    pub events: broadcast::Sender<ScanProgressEvent>,
    pub scan: LibraryScanConfig,
    pub scan_root: Option<StoragePath>,
    pub convert_job_tx: Option<tokio::sync::mpsc::Sender<i64>>,
    pub runtime:
        Option<std::sync::Arc<tokio::sync::RwLock<crate::services::app_settings::RuntimeSettings>>>,
}

/// Audio file queued during discovery with size/mtime from directory listing.
struct AudioScanEntry {
    path: StoragePath,
    size: u64,
    mtime: Option<String>,
}

#[derive(Clone)]
struct StorageAudioEntryCtx {
    scan_id: i64,
    pool: SqlitePool,
    storage: Arc<dyn LibraryStorage>,
    audio_total: usize,
    scan_deps: ScanDeps,
    counters: ScanProgressCounters,
    events: broadcast::Sender<ScanProgressEvent>,
    files_total: Arc<Mutex<Option<i64>>>,
    debug: bool,
}

async fn storage_list_dir_timed(
    storage: &Arc<dyn LibraryStorage>,
    path: &StoragePath,
) -> Result<Vec<crate::library::storage::StorageEntry>, ApiError> {
    let label = path.as_str();
    match tokio::time::timeout(STORAGE_LIST_DIR_TIMEOUT, storage.list_dir(path)).await {
        Ok(result) => result,
        Err(_) => Err(ApiError::Message(format!(
            "STORAGE_LIST_TIMEOUT: listing '{label}' exceeded {}s",
            STORAGE_LIST_DIR_TIMEOUT.as_secs()
        ))),
    }
}

async fn storage_read_bytes_capped(
    storage: &Arc<dyn LibraryStorage>,
    path: &StoragePath,
    max_bytes: usize,
) -> Result<Vec<u8>, ApiError> {
    if max_bytes == 0 {
        return Ok(Vec::new());
    }
    let label = path.as_str();
    let stream_future = storage.read_stream(path, 0, Some(max_bytes as u64));
    let mut stream = match tokio::time::timeout(STORAGE_FILE_OP_TIMEOUT, stream_future).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            return Err(ApiError::Message(format!(
                "STORAGE_READ_TIMEOUT: opening '{label}' exceeded {}s",
                STORAGE_FILE_OP_TIMEOUT.as_secs()
            )));
        }
    };
    let mut out = Vec::with_capacity(max_bytes.min(64 * 1024));
    while out.len() < max_bytes {
        let item = match tokio::time::timeout(STORAGE_READ_CHUNK_TIMEOUT, stream.next()).await {
            Ok(Some(item)) => item,
            Ok(None) => break,
            Err(_) => {
                return Err(ApiError::Message(format!(
                    "STORAGE_READ_TIMEOUT: '{label}' chunk exceeded {}s",
                    STORAGE_READ_CHUNK_TIMEOUT.as_secs()
                )));
            }
        };
        let chunk = item.map_err(|e| ApiError::Message(format!("storage read stream: {e}")))?;
        if chunk.is_empty() {
            continue;
        }
        let take = (max_bytes - out.len()).min(chunk.len());
        out.extend_from_slice(&chunk[..take]);
    }
    Ok(out)
}

/// Read tags from at most ~head (+ FLAC tail) bytes over SMB; never the full file.
async fn storage_read_tags_limited(
    storage: &Arc<dyn LibraryStorage>,
    path: &StoragePath,
    file_size: u64,
) -> Result<TrackTags, ApiError> {
    if file_size == 0 {
        return Err(ApiError::Message(format!(
            "storage scan empty file: {}",
            path.as_str()
        )));
    }
    let path_rel = path.as_str();
    let head_len = (file_size as usize).min(STORAGE_TAG_HEAD);
    let head = storage_read_bytes_capped(storage, path, head_len).await?;
    if tags::is_flac_path(path_rel) {
        if let Ok(tags) = tags::try_read_tags_lofty_bytes(&head, path_rel) {
            return Ok(tags);
        }
        let tail_len = tags::limited_tag_flac_tail_len(file_size);
        let tail_offset = file_size.saturating_sub(tail_len as u64);
        let tail_bytes = match tokio::time::timeout(
            STORAGE_FILE_OP_TIMEOUT,
            storage.read_at(path, tail_offset, tail_len),
        )
        .await
        {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(ApiError::Message(format!(
                    "STORAGE_READ_TIMEOUT: FLAC tail '{path_rel}' exceeded {}s",
                    STORAGE_FILE_OP_TIMEOUT.as_secs()
                )));
            }
        };
        return Ok(tags::read_tags_limited_bytes(
            head,
            Some(tail_bytes.to_vec()),
            path_rel,
        ));
    }
    Ok(tags::read_tags_limited_bytes(head, None, path_rel))
}

async fn flush_storage_scan_discovery(
    scan_id: i64,
    pool: &SqlitePool,
    entries_walked: i64,
    audio_seen: i64,
    counters: &ScanProgressCounters,
) -> Result<(), ApiError> {
    let estimate = entries_walked.max(audio_seen).max(1);
    *counters
        .files_total_final
        .lock()
        .expect("scan files_total lock poisoned") = Some(estimate);
    flush_scan_progress(
        scan_id,
        pool,
        entries_walked,
        counters.files_processed.load(Ordering::Relaxed),
        counters.files_indexed.load(Ordering::Relaxed),
        counters.files_total_final.as_ref(),
        &counters.events,
    )
    .await
}

async fn run_storage_scan(scan_id: i64, deps: StorageScanDeps) -> Result<(), ApiError> {
    let scan_root_display = deps
        .scan_root
        .as_ref()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    tracing::info!(
        scan_id,
        scan_root = %scan_root_display,
        "storage library scan started"
    );

    let counters = ScanProgressCounters {
        files_seen: Arc::new(AtomicI64::new(0)),
        files_processed: Arc::new(AtomicI64::new(0)),
        files_indexed: Arc::new(AtomicI64::new(0)),
        files_total_final: Arc::new(Mutex::new(None)),
        events: deps.events.clone(),
    };
    let entries_walked = Arc::new(AtomicI64::new(0));
    flush_storage_scan_discovery(scan_id, &deps.pool, 0, 0, &counters).await?;

    let mut dirs: VecDeque<StoragePath> = VecDeque::new();
    dirs.push_back(deps.scan_root.clone().unwrap_or_else(StoragePath::root));
    let mut visited = HashSet::new();
    let mut audio_entries: Vec<AudioScanEntry> = Vec::new();
    while let Some(dir) = dirs.pop_front() {
        if library_scan_runs::is_cancelled(&deps.pool, scan_id).await? {
            return Ok(());
        }
        let dir_key = dir.as_str().to_string();
        if !visited.insert(dir_key.clone()) {
            tracing::debug!(scan_id, dir = %dir_key, "storage scan skipping duplicate directory");
            continue;
        }
        tracing::info!(
            scan_id,
            dir = %dir.as_str(),
            pending_dirs = dirs.len(),
            "storage scan listing directory"
        );
        let entries = storage_list_dir_timed(&deps.storage, &dir).await?;
        let n_entries = entries.len();
        let walked =
            entries_walked.fetch_add(n_entries as i64, Ordering::Relaxed) + n_entries as i64;
        let audio_seen = counters.files_seen.load(Ordering::Relaxed);
        tracing::info!(
            scan_id,
            dir = %dir.as_str(),
            entries = n_entries,
            entries_walked = walked,
            audio_seen,
            "storage scan listed directory"
        );
        flush_storage_scan_discovery(scan_id, &deps.pool, walked, audio_seen, &counters).await?;

        let mut subdirs = 0usize;
        let mut audio_here = 0usize;
        for entry in entries {
            if entry.kind == StorageEntryKind::Directory {
                dirs.push_back(entry.path);
                subdirs += 1;
                continue;
            }
            if !is_audio_file(std::path::Path::new(entry.path.as_str())) {
                continue;
            }
            let Some(size) = entry.size else {
                tracing::warn!(
                    scan_id,
                    path = %entry.path.as_str(),
                    "storage scan audio entry missing size from listing, skipping"
                );
                continue;
            };
            audio_entries.push(AudioScanEntry {
                path: entry.path,
                size,
                mtime: entry.mtime,
            });
            audio_here += 1;
        }
        tracing::info!(
            scan_id,
            dir = %dir.as_str(),
            subdirs,
            audio_here,
            pending_dirs = dirs.len(),
            audio_queued = audio_entries.len(),
            "storage scan queued entries from directory"
        );
    }

    let audio_total = i64::try_from(audio_entries.len()).unwrap_or(i64::MAX);
    tracing::info!(
        scan_id,
        audio_files = audio_entries.len(),
        "storage scan discovery finished, processing audio"
    );
    *counters
        .files_total_final
        .lock()
        .expect("scan files_total lock poisoned") = Some(audio_total.max(1));
    counters.files_seen.store(0, Ordering::Relaxed);
    flush_scan_progress(
        scan_id,
        &deps.pool,
        0,
        0,
        0,
        counters.files_total_final.as_ref(),
        &deps.events,
    )
    .await?;

    let scan_deps = ScanDeps {
        pool: deps.pool.clone(),
        library_path: PathBuf::new(),
        events: deps.events.clone(),
        scan: deps.scan.clone(),
        scan_root: None,
        convert_job_tx: deps.convert_job_tx.clone(),
        runtime: deps.runtime.clone(),
    };

    let audio_count = audio_entries.len();
    let mut album_paths = HashSet::new();
    let discovered_paths: Vec<String> = audio_entries
        .iter()
        .map(|entry| entry.path.as_str().to_string())
        .collect();
    for entry in &audio_entries {
        if let Some(parent) = entry.path.parent() {
            let rel = parent.as_str();
            if !rel.is_empty() {
                album_paths.insert(rel.to_string());
            }
        }
    }

    // SMB client serializes I/O per session; >2 parallel readers mostly queue on op_serial.
    let smb_io_workers = deps.scan.process_workers.clamp(1, 2);
    let processing_workers = deps.scan.process_workers.clamp(1, 8);
    let smb_io = Arc::new(Semaphore::new(smb_io_workers));
    tracing::info!(
        scan_id,
        audio_total = audio_count,
        smb_io_workers,
        processing_workers,
        "storage scan processing audio (parallel)"
    );

    let entry_ctx = StorageAudioEntryCtx {
        scan_id,
        pool: deps.pool.clone(),
        storage: deps.storage.clone(),
        audio_total: audio_count,
        scan_deps,
        counters: counters.clone(),
        events: deps.events.clone(),
        files_total: counters.files_total_final.clone(),
        debug: deps.scan.debug,
    };
    let mut entries = audio_entries.into_iter();
    let mut tasks = tokio::task::JoinSet::new();
    let mut in_flight = 0usize;
    loop {
        while in_flight < processing_workers {
            if library_scan_runs::is_cancelled(&deps.pool, scan_id).await? {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                return Ok(());
            }
            let Some(entry) = entries.next() else {
                break;
            };
            let ctx = entry_ctx.clone();
            let smb_io = smb_io.clone();
            tasks.spawn(async move {
                let Ok(_permit) = smb_io.acquire_owned().await else {
                    tracing::error!(scan_id = ctx.scan_id, "storage scan SMB semaphore closed");
                    return;
                };
                if let Err(e) = process_storage_audio_entry(&ctx, entry).await {
                    tracing::error!(
                        scan_id = ctx.scan_id,
                        error = %e,
                        "storage scan file worker failed"
                    );
                }
            });
            in_flight += 1;
        }
        if in_flight == 0 {
            break;
        }
        tokio::select! {
            result = tasks.join_next() => {
                in_flight = in_flight.saturating_sub(1);
                if let Some(result) = result
                    && let Err(e) = result
                {
                    tracing::error!(scan_id, error = %e, "storage scan indexing task join failed");
                }
            }
            cancelled = library_scan_runs::is_cancelled(&deps.pool, scan_id) => {
                if cancelled? {
                    tasks.abort_all();
                    while tasks.join_next().await.is_some() {}
                    return Ok(());
                }
            }
        }
    }
    let processed = counters.files_processed.load(Ordering::Relaxed);
    if processed < audio_count as i64 {
        tracing::warn!(
            scan_id,
            processed,
            audio_total = audio_count,
            "storage scan finished indexing with fewer files than discovered"
        );
    }
    if library_scan_runs::is_cancelled(&deps.pool, scan_id).await? {
        return Ok(());
    }
    storage_scan_album_cover_pass(&deps.pool, &deps.storage, &album_paths).await?;
    if library_scan_runs::is_cancelled(&deps.pool, scan_id).await? {
        return Ok(());
    }
    storage_scan_prune_stale(&deps.pool, deps.scan_root.as_ref(), &discovered_paths).await?;

    let total = counters.files_seen.load(Ordering::Relaxed);
    flush_scan_progress(
        scan_id,
        &deps.pool,
        total,
        counters.files_processed.load(Ordering::Relaxed),
        counters.files_indexed.load(Ordering::Relaxed),
        counters.files_total_final.as_ref(),
        &deps.events,
    )
    .await?;
    if library_scan_runs::is_cancelled(&deps.pool, scan_id).await? {
        return Ok(());
    }
    library_scan_runs::finish_success(&deps.pool, scan_id).await?;
    if deps.runtime.is_some() {
        spawn_storage_hash_backfill(deps.pool.clone(), deps.storage.clone(), smb_io);
    }
    Ok(())
}

async fn storage_scan_prune_stale(
    pool: &SqlitePool,
    scan_root: Option<&StoragePath>,
    discovered_paths: &[String],
) -> Result<(), ApiError> {
    let scope = scan_root.and_then(|path| {
        let rel = path.as_str();
        if rel.is_empty() { None } else { Some(rel) }
    });
    let removed_tracks = tracks::delete_absent_in_scope(pool, scope, discovered_paths).await?;
    let removed_albums = albums::delete_empty_storage_albums_in_scope(pool, scope).await?;
    if removed_tracks > 0 || removed_albums > 0 {
        tracing::info!(
            scope = %scope.unwrap_or(""),
            removed_tracks,
            removed_albums,
            "storage scan pruned stale library rows"
        );
    }
    Ok(())
}

pub fn spawn_storage_hash_backfill(
    pool: SqlitePool,
    storage: Arc<dyn LibraryStorage>,
    smb_io: Arc<Semaphore>,
) {
    tokio::spawn(async move {
        if let Err(e) = run_storage_hash_backfill(&pool, storage, smb_io).await {
            tracing::warn!(error = %e, "storage file_hash backfill failed");
        }
    });
}

async fn storage_content_hash_xxh64(
    storage: &Arc<dyn LibraryStorage>,
    path: &StoragePath,
    file_size: u64,
) -> Result<String, ApiError> {
    if file_size == 0 {
        return Err(ApiError::Message(format!(
            "hash backfill empty file: {}",
            path.as_str()
        )));
    }
    let label = path.as_str();
    let stream_future = storage.read_stream(path, 0, Some(file_size));
    let mut stream = match tokio::time::timeout(STORAGE_FILE_OP_TIMEOUT, stream_future).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            return Err(ApiError::Message(format!(
                "STORAGE_READ_TIMEOUT: hash '{label}' exceeded {}s",
                STORAGE_FILE_OP_TIMEOUT.as_secs()
            )));
        }
    };
    let mut hasher = ContentXxh64::new();
    loop {
        let item = match tokio::time::timeout(STORAGE_READ_CHUNK_TIMEOUT, stream.next()).await {
            Ok(Some(item)) => item,
            Ok(None) => break,
            Err(_) => {
                return Err(ApiError::Message(format!(
                    "STORAGE_READ_TIMEOUT: hash chunk '{label}' exceeded {}s",
                    STORAGE_READ_CHUNK_TIMEOUT.as_secs()
                )));
            }
        };
        let chunk = item.map_err(|e| ApiError::Message(format!("hash read stream: {e}")))?;
        hasher.update(&chunk);
    }
    Ok(hasher.finish())
}

pub async fn run_storage_hash_backfill(
    pool: &SqlitePool,
    storage: Arc<dyn LibraryStorage>,
    smb_io: Arc<Semaphore>,
) -> Result<(), ApiError> {
    let mut after_id = 0;
    let mut total_seen = 0usize;
    loop {
        let rows =
            tracks::list_needing_file_hash_batch(pool, after_id, STORAGE_HASH_BACKFILL_BATCH)
                .await?;
        if rows.is_empty() {
            break;
        }
        total_seen += rows.len();
        tracing::info!(
            tracks = rows.len(),
            after_id,
            "storage file_hash backfill batch started"
        );
        let mut tasks = tokio::task::JoinSet::new();
        for row in rows {
            after_id = after_id.max(row.id);
            let pool = pool.clone();
            let storage = storage.clone();
            let sem = smb_io.clone();
            tasks.spawn(async move {
                let permit = sem
                    .acquire_owned()
                    .await
                    .map_err(|_| ApiError::Message("hash backfill semaphore closed".into()))?;
                let _permit = permit;
                process_storage_hash_backfill_row(&pool, storage, row).await
            });
        }
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(error = %e, "hash backfill row failed"),
                Err(e) => tracing::warn!(error = %e, "hash backfill task join failed"),
            }
        }
    }
    tracing::info!(tracks = total_seen, "storage file_hash backfill finished");
    Ok(())
}

async fn process_storage_hash_backfill_row(
    pool: &SqlitePool,
    storage: Arc<dyn LibraryStorage>,
    row: tracks::TrackHashBackfillRow,
) -> Result<(), ApiError> {
    let path = match StoragePath::parse(&row.path) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(track_id = row.id, path = %row.path, error = %e, "hash backfill skip");
            return Ok(());
        }
    };
    let Some(file_size) = row.file_size.filter(|s| *s > 0).map(|s| s as u64) else {
        tracing::warn!(track_id = row.id, path = %row.path, "hash backfill skip: missing size");
        return Ok(());
    };
    if file_size > STORAGE_MAX_READ_BYTES {
        tracing::warn!(
            track_id = row.id,
            path = %row.path,
            size = file_size,
            "hash backfill skip: oversized"
        );
        return Ok(());
    }
    match storage_content_hash_xxh64(&storage, &path, file_size).await {
        Ok(hash) => tracks::set_file_hash(pool, row.id, &hash).await,
        Err(e) => {
            tracing::warn!(
                track_id = row.id,
                path = %row.path,
                error = %e,
                "hash backfill read failed"
            );
            Ok(())
        }
    }
}

async fn flush_storage_audio_file_done(
    scan_id: i64,
    pool: &SqlitePool,
    counters: &ScanProgressCounters,
    events: &broadcast::Sender<ScanProgressEvent>,
    files_total: &Arc<Mutex<Option<i64>>>,
    audio_total: usize,
) -> Result<(), ApiError> {
    let processed = counters.files_processed.fetch_add(1, Ordering::Relaxed) + 1;
    counters.files_seen.store(processed, Ordering::Relaxed);
    counters.files_indexed.store(processed, Ordering::Relaxed);
    if (processed as usize).is_multiple_of(PROGRESS_EVERY) || processed as usize == audio_total {
        flush_scan_progress(
            scan_id,
            pool,
            processed,
            processed,
            processed,
            files_total,
            events,
        )
        .await?;
    }
    Ok(())
}

async fn process_storage_audio_entry(
    ctx: &StorageAudioEntryCtx,
    entry: AudioScanEntry,
) -> Result<(), ApiError> {
    let path_rel = entry.path.as_str().to_string();
    let file_size = entry.size;
    tracing::info!(
        scan_id = ctx.scan_id,
        path = %path_rel,
        size = file_size,
        audio_total = ctx.audio_total,
        "storage scan processing audio"
    );
    let work = process_storage_audio_entry_inner(
        ctx.scan_id,
        &ctx.pool,
        &ctx.storage,
        entry,
        &ctx.scan_deps,
        ctx.debug,
    );
    match tokio::time::timeout(STORAGE_FILE_INDEX_TIMEOUT, work).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!(
                scan_id = ctx.scan_id,
                path = %path_rel,
                error = %e,
                "storage scan skipping file after error"
            );
        }
        Err(_) => {
            tracing::warn!(
                scan_id = ctx.scan_id,
                path = %path_rel,
                timeout_secs = STORAGE_FILE_INDEX_TIMEOUT.as_secs(),
                size = file_size,
                "storage scan file timed out"
            );
        }
    }
    flush_storage_audio_file_done(
        ctx.scan_id,
        &ctx.pool,
        &ctx.counters,
        &ctx.events,
        &ctx.files_total,
        ctx.audio_total,
    )
    .await
}

async fn process_storage_audio_entry_inner(
    scan_id: i64,
    pool: &SqlitePool,
    storage: &Arc<dyn LibraryStorage>,
    entry: AudioScanEntry,
    scan_deps: &ScanDeps,
    debug: bool,
) -> Result<(), ApiError> {
    let path = entry.path;
    let path_rel = path.as_str().to_string();
    let file_size = entry.size;
    let file_mtime = entry.mtime;
    if file_size > STORAGE_MAX_READ_BYTES {
        tracing::warn!(
            scan_id,
            path = %path_rel,
            size = file_size,
            "storage scan skipping oversized file"
        );
        return Ok(());
    }
    let size_i64 = i64::try_from(file_size).ok();
    let album_path_rel = path
        .parent()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();

    if library_scan_runs::is_cancelled(pool, scan_id).await? {
        return Ok(());
    }

    if let Some((db_mtime, db_size)) = tracks::get_fingerprint_by_path(pool, &path_rel).await?
        && db_mtime.as_deref() == file_mtime.as_deref()
        && db_size.is_some()
        && db_size == size_i64
    {
        tracing::debug!(scan_id, path = %path_rel, "storage scan unchanged, skip");
        return Ok(());
    }

    tracing::info!(
        scan_id,
        path = %path_rel,
        size = file_size,
        "storage scan reading file"
    );
    if library_scan_runs::is_cancelled(pool, scan_id).await? {
        return Ok(());
    }
    let tags = storage_read_tags_limited(storage, &path, file_size).await?;
    if library_scan_runs::is_cancelled(pool, scan_id).await? {
        return Ok(());
    }
    let job = ScanIndexJob {
        path_rel: path_rel.clone(),
        album_path_rel,
        tags,
        file_mtime,
        file_hash: None,
        file_size: size_i64,
        cover_path: None,
    };
    persist_index(pool, job, scan_deps).await?;
    scan_debug!(debug, scan_id, path = %path_rel, "persisted storage track");
    Ok(())
}

async fn storage_scan_album_cover_pass(
    pool: &SqlitePool,
    storage: &Arc<dyn LibraryStorage>,
    album_paths: &HashSet<String>,
) -> Result<(), ApiError> {
    for album_path_rel in album_paths {
        let Some(album_id) = albums::id_by_path(pool, album_path_rel).await? else {
            continue;
        };
        let current = albums::get_by_id(pool, album_id)
            .await?
            .and_then(|a| a.cover_path);
        let work = ensure_album_cover_path_storage(
            pool,
            storage.as_ref(),
            album_id,
            Some(album_path_rel),
            current.as_deref(),
        );
        match tokio::time::timeout(STORAGE_LIST_DIR_TIMEOUT, work).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::warn!(
                    album = %album_path_rel,
                    error = %e,
                    "storage scan album cover pass failed"
                );
            }
            Err(_) => {
                tracing::warn!(
                    album = %album_path_rel,
                    timeout_secs = STORAGE_LIST_DIR_TIMEOUT.as_secs(),
                    "storage scan album cover pass timed out"
                );
            }
        }
    }
    Ok(())
}

pub async fn start_scan(
    pool: &SqlitePool,
    library_path: PathBuf,
    events: broadcast::Sender<ScanProgressEvent>,
    scan: LibraryScanConfig,
    scan_root: Option<PathBuf>,
    convert_job_tx: Option<tokio::sync::mpsc::Sender<i64>>,
    runtime: Option<
        std::sync::Arc<tokio::sync::RwLock<crate::services::app_settings::RuntimeSettings>>,
    >,
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
            convert_job_tx,
            runtime,
        },
    );
    Ok(scan_id)
}

/// Poll until the scan run is no longer `running` (success, failed, or cancelled).
pub async fn wait_scan_finished(pool: &SqlitePool, scan_id: i64) {
    loop {
        match library_scan_runs::get_by_id(pool, scan_id).await {
            Ok(Some(run)) if run.status == "running" => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            _ => break,
        }
    }
}

pub async fn start_scan_storage(
    pool: &SqlitePool,
    storage: Arc<dyn LibraryStorage>,
    events: broadcast::Sender<ScanProgressEvent>,
    scan: LibraryScanConfig,
    scan_root: Option<StoragePath>,
    convert_job_tx: Option<tokio::sync::mpsc::Sender<i64>>,
    runtime: Option<
        std::sync::Arc<tokio::sync::RwLock<crate::services::app_settings::RuntimeSettings>>,
    >,
) -> Result<i64, ApiError> {
    if library_scan_runs::has_running(pool).await? {
        return Err(ApiError::Message("SCAN_ALREADY_RUNNING".into()));
    }
    let scan_id = library_scan_runs::start(pool).await?;
    let deps = StorageScanDeps {
        pool: pool.clone(),
        storage,
        events,
        scan,
        scan_root,
        convert_job_tx,
        runtime,
    };
    tokio::spawn(async move {
        let pool = deps.pool.clone();
        match run_storage_scan(scan_id, deps).await {
            Ok(()) => {}
            Err(e) => {
                tracing::error!(scan_id, error = %e, "storage library scan failed");
                let _ = library_scan_runs::finish_failed(&pool, scan_id, &e.to_string()).await;
            }
        }
    });
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
    use crate::db::{artists, connect, convert_jobs, migrate};
    use crate::library::file_hash::content_hash_xxh64;
    use crate::library::storage::LocalStorage;
    use crate::services::app_settings::{RuntimeSettings, StorageSettings};
    use tempfile::TempDir;
    use tokio::sync::{RwLock, broadcast, mpsc};

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

    #[test]
    fn file_hash_sync_matches_buffer_hash() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("track.bin");
        let bytes = b"hash me in streaming chunks";
        std::fs::write(&path, bytes).unwrap();

        let hash = file_hash_sync(&path).unwrap().unwrap();
        assert_eq!(hash, content_hash_xxh64(bytes));
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

    async fn seed_storage_track(pool: &SqlitePool, album_path: &str, track_path: &str) -> i64 {
        let artist_id = artists::upsert_by_name(pool, "Seed Artist", None)
            .await
            .unwrap();
        let album_id = albums::upsert(
            pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: album_path,
                year: None,
                qobuz_album_id: None,
                path: Some(album_path),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        tracks::upsert(
            pool,
            tracks::TrackUpsert {
                album_id,
                title: track_path,
                track_number: None,
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: track_path,
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();
        album_id
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
                track_total: None,
                disc_total: None,
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
                convert_job_tx: None,
                runtime: None,
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
    async fn storage_scan_auto_enqueues_convert_job_with_relative_db_path() {
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
                track_total: None,
                disc_total: None,
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
        let (convert_tx, mut convert_rx) = mpsc::channel(1);
        let mut runtime = RuntimeSettings::default();
        runtime.converter.auto_enabled = true;
        runtime.storage = StorageSettings::local(dir.path().display().to_string());
        let runtime = std::sync::Arc::new(RwLock::new(runtime));
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_storage_scan(
            scan_id,
            StorageScanDeps {
                pool: pool.clone(),
                storage: std::sync::Arc::new(LocalStorage::new(dir.path())),
                events,
                scan: scan_cfg_1_1(),
                scan_root: None,
                convert_job_tx: Some(convert_tx),
                runtime: Some(runtime),
            },
        )
        .await
        .unwrap();

        let run = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.status, "success", "run: {run:?}");
        let album = albums::list_keyset(
            &pool,
            albums::AlbumsListParams {
                sort: albums::AlbumsSort::Title,
                order: crate::api::SortOrder::Asc,
                limit: 10,
                q: None,
                cursor: None,
            },
        )
        .await
        .unwrap()
        .items
        .pop()
        .unwrap();
        let track = tracks::list_by_album(&pool, album.id)
            .await
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(track.path, "Artist A/Album One/01.wav");
        assert!(
            !track
                .path
                .starts_with(dir.path().to_string_lossy().as_ref())
        );

        let job_id = convert_jobs::next_queued_id(&pool).await.unwrap().unwrap();
        let job = convert_jobs::get_by_id(&pool, job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.status, "queued");
        assert_eq!(job.trigger, "auto");
        assert_eq!(job.files_total, 1);
        assert_eq!(convert_rx.try_recv().unwrap(), 0);
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
                track_total: None,
                disc_total: None,
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
                track_total: None,
                disc_total: None,
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
                convert_job_tx: None,
                runtime: None,
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
        let distinct_paths: (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT path) FROM tracks")
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
                track_total: None,
                disc_total: None,
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
            convert_job_tx: None,
            runtime: None,
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

    struct MetadataCountingStorage {
        inner: std::sync::Arc<LocalStorage>,
        metadata_calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl crate::library::storage::LibraryStorage for MetadataCountingStorage {
        async fn metadata(
            &self,
            path: &StoragePath,
        ) -> Result<crate::library::storage::StorageMetadata, ApiError> {
            self.metadata_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.metadata(path).await
        }

        async fn list_dir(
            &self,
            path: &StoragePath,
        ) -> Result<Vec<crate::library::storage::StorageEntry>, ApiError> {
            self.inner.list_dir(path).await
        }

        async fn read(&self, path: &StoragePath) -> Result<bytes::Bytes, ApiError> {
            self.inner.read(path).await
        }

        async fn read_at(
            &self,
            path: &StoragePath,
            offset: u64,
            len: usize,
        ) -> Result<bytes::Bytes, ApiError> {
            self.inner.read_at(path, offset, len).await
        }

        async fn read_stream(
            &self,
            path: &StoragePath,
            offset: u64,
            len: Option<u64>,
        ) -> Result<crate::library::storage::StorageByteStream, ApiError> {
            self.inner.read_stream(path, offset, len).await
        }

        async fn atomic_write(
            &self,
            path: &StoragePath,
            bytes: bytes::Bytes,
        ) -> Result<(), ApiError> {
            self.inner.atomic_write(path, bytes).await
        }

        async fn create_dir_all(&self, path: &StoragePath) -> Result<(), ApiError> {
            self.inner.create_dir_all(path).await
        }

        async fn rename(&self, from: &StoragePath, to: &StoragePath) -> Result<(), ApiError> {
            self.inner.rename(from, to).await
        }

        async fn delete(&self, path: &StoragePath) -> Result<(), ApiError> {
            self.inner.delete(path).await
        }
    }

    #[tokio::test]
    async fn storage_scan_uses_listing_mtime_without_per_file_metadata() {
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
                track_total: None,
                disc_total: None,
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
        let inner = std::sync::Arc::new(LocalStorage::new(dir.path()));
        let counting = std::sync::Arc::new(MetadataCountingStorage {
            inner: inner.clone(),
            metadata_calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let storage: std::sync::Arc<dyn crate::library::storage::LibraryStorage> = counting.clone();
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_storage_scan(
            scan_id,
            StorageScanDeps {
                pool: pool.clone(),
                storage,
                events,
                scan: scan_cfg_1_1(),
                scan_root: None,
                convert_job_tx: None,
                runtime: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            counting
                .metadata_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "storage scan should use list_dir mtime/size, not per-file metadata()"
        );
        let run = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.files_indexed, 1);
    }

    #[tokio::test]
    async fn storage_scan_prunes_stale_tracks_after_successful_full_scan() {
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
                track_total: None,
                disc_total: None,
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
        let stale_album_id = seed_storage_track(
            &pool,
            "Old Artist/Old Album",
            "Old Artist/Old Album/01.flac",
        )
        .await;
        let (events, _) = broadcast::channel(8);
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_storage_scan(
            scan_id,
            StorageScanDeps {
                pool: pool.clone(),
                storage: std::sync::Arc::new(LocalStorage::new(dir.path())),
                events,
                scan: scan_cfg_1_1(),
                scan_root: None,
                convert_job_tx: None,
                runtime: None,
            },
        )
        .await
        .unwrap();

        let stale_tracks = tracks::list_by_album(&pool, stale_album_id).await.unwrap();
        assert!(stale_tracks.is_empty());
        assert!(
            albums::get_by_id(&pool, stale_album_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn storage_scan_prunes_only_inside_scoped_scan_root() {
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
                track_total: None,
                disc_total: None,
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
        let scoped_album_id =
            seed_storage_track(&pool, "Artist A/Album One", "Artist A/Album One/stale.flac").await;
        let outside_album_id =
            seed_storage_track(&pool, "Other Artist/Album", "Other Artist/Album/stale.flac").await;
        let (events, _) = broadcast::channel(8);
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_storage_scan(
            scan_id,
            StorageScanDeps {
                pool: pool.clone(),
                storage: std::sync::Arc::new(LocalStorage::new(dir.path())),
                events,
                scan: scan_cfg_1_1(),
                scan_root: Some(StoragePath::parse("Artist A/Album One").unwrap()),
                convert_job_tx: None,
                runtime: None,
            },
        )
        .await
        .unwrap();

        let scoped_tracks = tracks::list_by_album(&pool, scoped_album_id).await.unwrap();
        assert_eq!(scoped_tracks.len(), 1);
        assert_eq!(scoped_tracks[0].path, "Artist A/Album One/01.wav");
        let outside_tracks = tracks::list_by_album(&pool, outside_album_id)
            .await
            .unwrap();
        assert_eq!(outside_tracks.len(), 1);
        assert_eq!(outside_tracks[0].path, "Other Artist/Album/stale.flac");
    }

    #[tokio::test]
    async fn failed_storage_scan_does_not_prune_existing_tracks() {
        struct FailingListStorage;

        #[async_trait::async_trait]
        impl crate::library::storage::LibraryStorage for FailingListStorage {
            async fn metadata(
                &self,
                _path: &StoragePath,
            ) -> Result<crate::library::storage::StorageMetadata, ApiError> {
                Err(ApiError::Message("boom".into()))
            }

            async fn list_dir(
                &self,
                _path: &StoragePath,
            ) -> Result<Vec<crate::library::storage::StorageEntry>, ApiError> {
                Err(ApiError::Message("boom".into()))
            }

            async fn read(&self, _path: &StoragePath) -> Result<bytes::Bytes, ApiError> {
                Err(ApiError::Message("boom".into()))
            }

            async fn read_at(
                &self,
                _path: &StoragePath,
                _offset: u64,
                _len: usize,
            ) -> Result<bytes::Bytes, ApiError> {
                Err(ApiError::Message("boom".into()))
            }

            async fn read_stream(
                &self,
                _path: &StoragePath,
                _offset: u64,
                _len: Option<u64>,
            ) -> Result<crate::library::storage::StorageByteStream, ApiError> {
                Err(ApiError::Message("boom".into()))
            }

            async fn atomic_write(
                &self,
                _path: &StoragePath,
                _bytes: bytes::Bytes,
            ) -> Result<(), ApiError> {
                Err(ApiError::Message("boom".into()))
            }

            async fn create_dir_all(&self, _path: &StoragePath) -> Result<(), ApiError> {
                Err(ApiError::Message("boom".into()))
            }

            async fn rename(&self, _from: &StoragePath, _to: &StoragePath) -> Result<(), ApiError> {
                Err(ApiError::Message("boom".into()))
            }

            async fn delete(&self, _path: &StoragePath) -> Result<(), ApiError> {
                Err(ApiError::Message("boom".into()))
            }
        }

        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let album_id =
            seed_storage_track(&pool, "Artist A/Album One", "Artist A/Album One/stale.flac").await;
        let (events, _) = broadcast::channel(8);
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        let err = run_storage_scan(
            scan_id,
            StorageScanDeps {
                pool: pool.clone(),
                storage: std::sync::Arc::new(FailingListStorage),
                events,
                scan: scan_cfg_1_1(),
                scan_root: None,
                convert_job_tx: None,
                runtime: None,
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("boom"));
        let rows = tracks::list_by_album(&pool, album_id).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert!(albums::get_by_id(&pool, album_id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn storage_hash_backfill_writes_xxh64_digest() {
        let dir = TempDir::new().unwrap();
        let album_dir = dir.path().join("Artist A").join("Album One");
        let track_path = write_test_wav_with_tags(
            &album_dir,
            "01.wav",
            tags::TrackTags {
                title: "Song".into(),
                artist: "Artist A".into(),
                album: "Album One".into(),
                track_number: Some(1),
                year: Some(2020),
                disc_number: None,
                track_total: None,
                disc_total: None,
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
        let artist_id = artists::upsert_by_name(&pool, "Artist A", None)
            .await
            .unwrap();
        let album_id = albums::upsert(
            &pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album One",
                year: Some(2020),
                qobuz_album_id: None,
                path: Some("Artist A/Album One"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let file_size = std::fs::metadata(&track_path).unwrap().len() as i64;
        tracks::upsert(
            &pool,
            tracks::TrackUpsert {
                album_id,
                title: "Song",
                track_number: Some(1),
                year: Some(2020),
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: "Artist A/Album One/01.wav",
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: Some(file_size),
            },
        )
        .await
        .unwrap();

        let storage: std::sync::Arc<dyn crate::library::storage::LibraryStorage> =
            std::sync::Arc::new(LocalStorage::new(dir.path()));
        run_storage_hash_backfill(&pool, storage, Arc::new(Semaphore::new(2)))
            .await
            .unwrap();

        let track = tracks::list_by_album(&pool, album_id)
            .await
            .unwrap()
            .pop()
            .unwrap();
        let hash = track.file_hash.expect("hash after backfill");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        let file_bytes = std::fs::read(&track_path).unwrap();
        assert_eq!(hash, content_hash_xxh64(&file_bytes));
    }

    #[tokio::test]
    async fn storage_hash_backfill_skips_bad_rows_and_continues_later_ids() {
        let dir = TempDir::new().unwrap();
        let album_dir = dir.path().join("Artist A").join("Album One");
        let good_path = write_test_wav_with_tags(
            &album_dir,
            "01.wav",
            tags::TrackTags {
                title: "Song".into(),
                artist: "Artist A".into(),
                album: "Album One".into(),
                track_number: Some(1),
                year: Some(2020),
                disc_number: None,
                track_total: None,
                disc_total: None,
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
        let artist_id = artists::upsert_by_name(&pool, "Artist A", None)
            .await
            .unwrap();
        let album_id = albums::upsert(
            &pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album One",
                year: Some(2020),
                qobuz_album_id: None,
                path: Some("Artist A/Album One"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        for (path, size) in [
            ("bad/../escape.flac", Some(10)),
            ("Artist A/Album One/missing-size.flac", None),
            ("Artist A/Album One/zero.flac", Some(0)),
            (
                "Artist A/Album One/oversized.flac",
                Some((STORAGE_MAX_READ_BYTES + 1) as i64),
            ),
            (
                "Artist A/Album One/01.wav",
                Some(std::fs::metadata(&good_path).unwrap().len() as i64),
            ),
        ] {
            tracks::upsert(
                &pool,
                tracks::TrackUpsert {
                    album_id,
                    title: path,
                    track_number: None,
                    year: None,
                    disc_number: None,
                    genre: None,
                    qobuz_track_id: None,
                    path,
                    duration_sec: None,
                    file_mtime: None,
                    file_hash: None,
                    file_size: size,
                },
            )
            .await
            .unwrap();
        }

        let storage: std::sync::Arc<dyn crate::library::storage::LibraryStorage> =
            std::sync::Arc::new(LocalStorage::new(dir.path()));
        run_storage_hash_backfill(&pool, storage, Arc::new(Semaphore::new(2)))
            .await
            .unwrap();

        let rows = tracks::list_by_album(&pool, album_id).await.unwrap();
        let hashed: Vec<_> = rows
            .iter()
            .filter(|row| row.file_hash.is_some())
            .map(|row| row.path.as_str())
            .collect();
        assert_eq!(hashed, vec!["Artist A/Album One/01.wav"]);
    }

    #[tokio::test]
    async fn storage_scan_applies_album_cover_after_indexing() {
        let dir = TempDir::new().unwrap();
        let album_dir = dir.path().join("Artist A").join("Album One");
        std::fs::create_dir_all(&album_dir).unwrap();
        std::fs::write(album_dir.join("cover.jpg"), b"cover-bytes").unwrap();
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
                track_total: None,
                disc_total: None,
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
        run_storage_scan(
            scan_id,
            StorageScanDeps {
                pool: pool.clone(),
                storage: std::sync::Arc::new(LocalStorage::new(dir.path())),
                events,
                scan: scan_cfg_1_1(),
                scan_root: None,
                convert_job_tx: None,
                runtime: None,
            },
        )
        .await
        .unwrap();

        let album = albums::list_keyset(
            &pool,
            albums::AlbumsListParams {
                sort: albums::AlbumsSort::Title,
                order: crate::api::SortOrder::Asc,
                limit: 1,
                q: None,
                cursor: None,
            },
        )
        .await
        .unwrap()
        .items
        .pop()
        .unwrap();
        assert_eq!(
            album.cover_path.as_deref(),
            Some("Artist A/Album One/cover.jpg")
        );
    }

    #[tokio::test]
    async fn storage_scan_counts_skipped_oversized_toward_files_total() {
        let dir = TempDir::new().unwrap();
        let album_dir = dir.path().join("Artist A").join("Album One");
        std::fs::create_dir_all(&album_dir).unwrap();
        std::fs::write(album_dir.join("huge.flac"), b"not-really-huge").unwrap();

        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let (events, _) = broadcast::channel(8);
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_storage_scan(
            scan_id,
            StorageScanDeps {
                pool: pool.clone(),
                storage: std::sync::Arc::new(OversizedListingStorage::new(
                    dir.path(),
                    STORAGE_MAX_READ_BYTES + 1,
                )),
                events,
                scan: scan_cfg_1_1(),
                scan_root: None,
                convert_job_tx: None,
                runtime: None,
            },
        )
        .await
        .unwrap();

        let run = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.status, "success");
        assert_eq!(run.files_total, 1);
        assert_eq!(run.files_processed, 1);
    }

    /// Reports a single audio file larger than the scan cap from listing metadata.
    struct OversizedListingStorage {
        inner: LocalStorage,
        reported_size: u64,
    }

    impl OversizedListingStorage {
        fn new(root: &std::path::Path, reported_size: u64) -> Self {
            Self {
                inner: LocalStorage::new(root),
                reported_size,
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::library::storage::LibraryStorage for OversizedListingStorage {
        async fn metadata(
            &self,
            path: &StoragePath,
        ) -> Result<crate::library::storage::StorageMetadata, ApiError> {
            self.inner.metadata(path).await
        }

        async fn list_dir(
            &self,
            path: &StoragePath,
        ) -> Result<Vec<crate::library::storage::StorageEntry>, ApiError> {
            let mut entries = self.inner.list_dir(path).await?;
            for entry in &mut entries {
                if entry.kind == crate::library::storage::StorageEntryKind::File {
                    entry.size = Some(self.reported_size);
                }
            }
            Ok(entries)
        }

        async fn read(&self, path: &StoragePath) -> Result<bytes::Bytes, ApiError> {
            self.inner.read(path).await
        }

        async fn read_at(
            &self,
            path: &StoragePath,
            offset: u64,
            len: usize,
        ) -> Result<bytes::Bytes, ApiError> {
            self.inner.read_at(path, offset, len).await
        }

        async fn read_stream(
            &self,
            path: &StoragePath,
            offset: u64,
            len: Option<u64>,
        ) -> Result<crate::library::storage::StorageByteStream, ApiError> {
            self.inner.read_stream(path, offset, len).await
        }

        async fn atomic_write(
            &self,
            path: &StoragePath,
            bytes: bytes::Bytes,
        ) -> Result<(), ApiError> {
            self.inner.atomic_write(path, bytes).await
        }

        async fn create_dir_all(&self, path: &StoragePath) -> Result<(), ApiError> {
            self.inner.create_dir_all(path).await
        }

        async fn rename(&self, from: &StoragePath, to: &StoragePath) -> Result<(), ApiError> {
            self.inner.rename(from, to).await
        }

        async fn delete(&self, path: &StoragePath) -> Result<(), ApiError> {
            self.inner.delete(path).await
        }
    }

    #[tokio::test]
    async fn storage_scan_indexes_with_null_file_hash() {
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
                track_total: None,
                disc_total: None,
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
        run_storage_scan(
            scan_id,
            StorageScanDeps {
                pool: pool.clone(),
                storage: std::sync::Arc::new(LocalStorage::new(dir.path())),
                events,
                scan: scan_cfg_1_1(),
                scan_root: None,
                convert_job_tx: None,
                runtime: None,
            },
        )
        .await
        .unwrap();

        let track = tracks::list_by_album(
            &pool,
            albums::list_keyset(
                &pool,
                albums::AlbumsListParams {
                    sort: albums::AlbumsSort::Title,
                    order: crate::api::SortOrder::Asc,
                    limit: 1,
                    q: None,
                    cursor: None,
                },
            )
            .await
            .unwrap()
            .items[0]
                .id,
        )
        .await
        .unwrap()
        .pop()
        .unwrap();
        assert!(track.file_hash.is_none());
        assert!(track.file_mtime.is_some());
        assert!(track.file_size.is_some());
    }

    #[tokio::test]
    async fn storage_scan_skips_unchanged_when_mtime_and_size_match() {
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
                track_total: None,
                disc_total: None,
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
        let storage: std::sync::Arc<dyn crate::library::storage::LibraryStorage> =
            std::sync::Arc::new(LocalStorage::new(dir.path()));
        let storage_deps = || StorageScanDeps {
            pool: pool.clone(),
            storage: storage.clone(),
            events: events.clone(),
            scan: scan_cfg_1_1(),
            scan_root: None,
            convert_job_tx: None,
            runtime: None,
        };

        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_storage_scan(scan_id, storage_deps()).await.unwrap();
        let first = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.files_indexed, 1);
        let track = tracks::list_by_album(
            &pool,
            albums::list_keyset(
                &pool,
                albums::AlbumsListParams {
                    sort: albums::AlbumsSort::Title,
                    order: crate::api::SortOrder::Asc,
                    limit: 1,
                    q: None,
                    cursor: None,
                },
            )
            .await
            .unwrap()
            .items[0]
                .id,
        )
        .await
        .unwrap()
        .pop()
        .unwrap();
        assert!(track.file_mtime.is_some());
        assert!(track.file_size.is_some());

        let scan_id2 = library_scan_runs::start(&pool).await.unwrap();
        run_storage_scan(scan_id2, storage_deps()).await.unwrap();
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
                track_total: None,
                disc_total: None,
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
                convert_job_tx: None,
                runtime: None,
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

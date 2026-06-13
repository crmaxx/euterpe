use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::AppConfig;
use crate::db::{albums, library_scan_runs, tracks};
use crate::error::ApiError;
use crate::library::storage::{self, StoragePath};
use crate::services::app_settings::{RuntimeSettingsHandle, StorageLocation};

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(1500);
const PENDING_SCAN_RETRY: Duration = Duration::from_secs(5);
const BACKOFF_STEPS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(60),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageWatchState {
    Disabled,
    Connected,
    Degraded,
    Reconnecting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageWatchStatus {
    pub state: StorageWatchState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
}

impl StorageWatchStatus {
    pub fn disabled() -> Self {
        Self {
            state: StorageWatchState::Disabled,
            degraded_reason: None,
        }
    }

    fn connected() -> Self {
        Self {
            state: StorageWatchState::Connected,
            degraded_reason: None,
        }
    }

    fn degraded(reason: impl Into<String>) -> Self {
        Self {
            state: StorageWatchState::Degraded,
            degraded_reason: Some(reason.into()),
        }
    }

    fn reconnecting(reason: impl Into<String>) -> Self {
        Self {
            state: StorageWatchState::Reconnecting,
            degraded_reason: Some(reason.into()),
        }
    }
}

#[derive(Clone)]
pub struct StorageWatchDeps {
    pub pool: sqlx::SqlitePool,
    pub config: Arc<AppConfig>,
    pub runtime: RuntimeSettingsHandle,
    pub scan_events: tokio::sync::broadcast::Sender<crate::api::ScanProgressEvent>,
    pub convert_job_tx: tokio::sync::mpsc::Sender<i64>,
}

pub struct StorageWatchHandle {
    status: Arc<RwLock<StorageWatchStatus>>,
    task: Arc<RwLock<Option<WatchTask>>>,
    deps: StorageWatchDeps,
}

struct WatchTask {
    cancel: CancellationToken,
    handle: JoinHandle<()>,
}

impl Clone for StorageWatchHandle {
    fn clone(&self) -> Self {
        Self {
            status: self.status.clone(),
            task: self.task.clone(),
            deps: self.deps.clone(),
        }
    }
}

impl StorageWatchHandle {
    pub fn new(deps: StorageWatchDeps) -> Self {
        Self {
            status: Arc::new(RwLock::new(StorageWatchStatus::disabled())),
            task: Arc::new(RwLock::new(None)),
            deps,
        }
    }

    pub async fn status(&self) -> StorageWatchStatus {
        self.status.read().await.clone()
    }

    /// Stops ChangeNotify while a library scan walks the tree (Samba often blocks `list_dir`).
    pub fn pause_for_scan(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.stop_current().await;
            *self.status.write().await = StorageWatchStatus::disabled();
            tracing::info!("SMB storage watch paused for library scan");
        })
    }

    pub fn restart(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.stop_current().await;
            let location = self.deps.runtime.read().await.storage.library.clone();
            match location {
                Some(StorageLocation::Smb { .. }) => {
                    let deps = self.deps.clone();
                    let status = self.status.clone();
                    let handle = self.clone();
                    let cancel = CancellationToken::new();
                    let task_cancel = cancel.clone();
                    *status.write().await =
                        StorageWatchStatus::reconnecting("starting SMB ChangeNotify watcher");
                    let task = tokio::spawn(async move {
                        watch_loop(deps, status, handle, task_cancel).await;
                    });
                    *self.task.write().await = Some(WatchTask {
                        cancel,
                        handle: task,
                    });
                }
                Some(StorageLocation::Local { .. }) | None => {
                    *self.status.write().await = StorageWatchStatus::disabled();
                }
            }
        })
    }

    async fn stop_current(&self) {
        if let Some(task) = self.task.write().await.take() {
            task.cancel.cancel();
            task.handle.abort();
        }
    }
}

async fn watch_loop(
    deps: StorageWatchDeps,
    status: Arc<RwLock<StorageWatchStatus>>,
    handle: StorageWatchHandle,
    cancel: CancellationToken,
) {
    let mut backoff_idx = 0usize;
    loop {
        if cancel.is_cancelled() {
            return;
        }
        match watch_once(&deps, &status, &handle, cancel.clone()).await {
            Ok(()) => {
                if cancel.is_cancelled() {
                    return;
                }
                backoff_idx = 0;
                *status.write().await =
                    StorageWatchStatus::reconnecting("SMB watcher disconnected");
            }
            Err(error) => {
                if cancel.is_cancelled() {
                    return;
                }
                let reason = error.to_string();
                *status.write().await = StorageWatchStatus::degraded(reason.clone());
                tracing::warn!(error = %reason, "SMB storage watcher degraded");
            }
        }
        let delay = BACKOFF_STEPS[backoff_idx.min(BACKOFF_STEPS.len() - 1)];
        backoff_idx = (backoff_idx + 1).min(BACKOFF_STEPS.len() - 1);
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(delay) => {}
        }
        *status.write().await = StorageWatchStatus::reconnecting("reconnecting SMB watcher");
    }
}

async fn watch_once(
    deps: &StorageWatchDeps,
    status: &Arc<RwLock<StorageWatchStatus>>,
    handle: &StorageWatchHandle,
    cancel: CancellationToken,
) -> Result<(), ApiError> {
    let location = deps.runtime.read().await.storage.library.clone();
    let Some(StorageLocation::Smb {
        host,
        port,
        share,
        path,
        username,
        password_encrypted,
        workgroup,
    }) = location
    else {
        *status.write().await = StorageWatchStatus::disabled();
        return Ok(());
    };

    let password = match password_encrypted {
        Some(value) => deps
            .config
            .master_key
            .as_ref()
            .ok_or_else(|| {
                ApiError::Message("EUTERPE_MASTER_KEY is required for SMB watch".into())
            })?
            .decrypt(&value)?,
        None => String::new(),
    };
    let location = euterpe_smb::SmbShareLocation {
        host,
        port,
        share,
        path,
    };
    let username = euterpe_smb::format_smb_username(
        workgroup.as_deref(),
        username.as_deref().unwrap_or_default(),
    );
    let credentials = euterpe_smb::SmbCredentials { username, password };
    let root_path = euterpe_smb::normalize_remote_path(&location.path);
    let mut stream = euterpe_smb::SmbStorageClient::new()
        .watch_directory(&location, &credentials, true)
        .await
        .map_err(|e| ApiError::Message(format!("SMB_CHANGE_NOTIFY_FAILED: {e}")))?;
    let (scan_tx, scan_rx) = mpsc::channel(256);
    let scan_deps = deps.clone();
    let scan_handle = handle.clone();
    let scan_cancel = cancel.clone();
    let scan_task = tokio::spawn(async move {
        if let Err(error) = run_debounce_worker(scan_deps, scan_rx, scan_handle, scan_cancel).await
        {
            tracing::warn!(error = %error, "storage watch debounce worker failed");
        }
    });
    *status.write().await = StorageWatchStatus::connected();
    loop {
        let item = tokio::select! {
            _ = cancel.cancelled() => break,
            item = stream.next() => item,
        };
        let Some(item) = item else {
            break;
        };
        let event =
            item.map_err(|e| ApiError::Message(format!("SMB_CHANGE_NOTIFY_FAILED: {e}")))?;
        let change = watch_event_change(&root_path, &event);
        if scan_tx.send(change).await.is_err() {
            break;
        }
    }
    drop(scan_tx);
    if let Err(error) = scan_task.await {
        tracing::warn!(error = %error, "storage watch debounce worker join failed");
    }
    Ok(())
}

async fn run_debounce_worker(
    deps: StorageWatchDeps,
    mut rx: mpsc::Receiver<PendingWatchChange>,
    watch: StorageWatchHandle,
    cancel: CancellationToken,
) -> Result<(), ApiError> {
    loop {
        let first = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            item = rx.recv() => item,
        };
        let Some(first) = first else {
            return Ok(());
        };
        let mut roots = Vec::new();
        let mut prunes = Vec::new();
        push_pending_change(first, &mut roots, &mut prunes);
        let delay = tokio::time::sleep(DEBOUNCE_WINDOW);
        tokio::pin!(delay);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                maybe_root = rx.recv() => {
                    match maybe_root {
                        Some(change) => push_pending_change(change, &mut roots, &mut prunes),
                        None => break,
                    }
                }
                _ = &mut delay => break,
            }
        }
        let scan_requested = !roots.is_empty();
        schedule_debounced_changes(
            &deps,
            &watch,
            scan_requested,
            coalesce_scan_roots(roots),
            prunes,
            &cancel,
        )
        .await?;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingWatchChange {
    Scan(Option<StoragePath>),
    Prune(StoragePath),
}

fn push_pending_change(
    change: PendingWatchChange,
    roots: &mut Vec<Option<StoragePath>>,
    prunes: &mut Vec<StoragePath>,
) {
    match change {
        PendingWatchChange::Scan(root) => roots.push(root),
        PendingWatchChange::Prune(path) => prunes.push(path),
    }
}

fn coalesce_scan_roots(roots: Vec<Option<StoragePath>>) -> Option<StoragePath> {
    let mut roots = roots.into_iter();
    let first = roots.next().flatten()?;
    for root in roots {
        match root {
            Some(root) if root == first => {}
            _ => return None,
        }
    }
    Some(first)
}

fn watch_event_scan_root(library_root: &str, event_path: &str) -> Option<StoragePath> {
    let rel_path = watch_event_storage_path(library_root, event_path)??;
    let looks_like_file = std::path::Path::new(rel_path.as_str())
        .extension()
        .and_then(|v| v.to_str())
        .is_some();
    if looks_like_file {
        rel_path.parent()
    } else {
        Some(rel_path)
    }
}

fn watch_event_change(
    library_root: &str,
    event: &euterpe_smb::SmbWatchEvent,
) -> PendingWatchChange {
    match event.action {
        euterpe_smb::SmbWatchAction::Removed | euterpe_smb::SmbWatchAction::RenamedOld => {
            watch_event_storage_path(library_root, &event.path)
                .flatten()
                .map(PendingWatchChange::Prune)
                .unwrap_or(PendingWatchChange::Scan(None))
        }
        euterpe_smb::SmbWatchAction::Created
        | euterpe_smb::SmbWatchAction::Modified
        | euterpe_smb::SmbWatchAction::RenamedNew => {
            PendingWatchChange::Scan(watch_event_scan_root(library_root, &event.path))
        }
    }
}

fn watch_event_storage_path(library_root: &str, event_path: &str) -> Option<Option<StoragePath>> {
    let rel = strip_watch_root(library_root, event_path)?;
    if rel.is_empty() {
        return Some(None);
    }
    Some(StoragePath::parse(&rel).ok())
}

fn strip_watch_root(library_root: &str, event_path: &str) -> Option<String> {
    let root = euterpe_smb::normalize_remote_path(library_root);
    let event = euterpe_smb::normalize_remote_path(event_path);
    if root.is_empty() {
        return Some(event);
    }
    if event == root {
        return Some(String::new());
    }
    event
        .strip_prefix(&format!("{root}/"))
        .map(ToString::to_string)
        .or(Some(event))
}

pub async fn schedule_debounced_scan(
    deps: &StorageWatchDeps,
    watch: &StorageWatchHandle,
    path: Option<StoragePath>,
) -> Result<(), ApiError> {
    schedule_debounced_changes(
        deps,
        watch,
        true,
        path,
        Vec::new(),
        &CancellationToken::new(),
    )
    .await
}

async fn schedule_debounced_changes(
    deps: &StorageWatchDeps,
    watch: &StorageWatchHandle,
    scan_requested: bool,
    path: Option<StoragePath>,
    prunes: Vec<StoragePath>,
    cancel: &CancellationToken,
) -> Result<(), ApiError> {
    for prune in prunes {
        let deleted = prune_removed_watch_path(&deps.pool, &prune).await?;
        if deleted > 0 {
            tracing::info!(
                path = %prune.as_str(),
                deleted,
                "storage watch pruned stale track rows"
            );
        }
    }
    if !scan_requested || cancel.is_cancelled() {
        return Ok(());
    }
    while library_scan_runs::has_running(&deps.pool).await? {
        if cancel.is_cancelled() {
            return Ok(());
        }
        tracing::info!("storage watch rescan pending: scan already running");
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            _ = tokio::time::sleep(PENDING_SCAN_RETRY) => {}
        }
    }
    let location = deps.runtime.read().await.storage.library.clone();
    let Some(location) = location else {
        return Ok(());
    };
    let storage = storage::storage_from_location(&location, deps.config.master_key.as_ref())?;
    let scan_cfg = deps
        .runtime
        .read()
        .await
        .library_scan_config(deps.config.debug)?;
    watch.pause_for_scan().await;
    let scan_id = match crate::services::library_scan::start_scan_storage(
        &deps.pool,
        storage,
        deps.scan_events.clone(),
        scan_cfg,
        path,
        Some(deps.convert_job_tx.clone()),
        Some(deps.runtime.clone()),
    )
    .await
    {
        Ok(scan_id) => scan_id,
        Err(error) => {
            watch.restart().await;
            return Err(error);
        }
    };
    crate::services::library_scan::wait_scan_finished(&deps.pool, scan_id).await;
    watch.restart().await;
    Ok(())
}

async fn prune_removed_watch_path(
    pool: &sqlx::SqlitePool,
    path: &StoragePath,
) -> Result<u64, ApiError> {
    let looks_like_file = std::path::Path::new(path.as_str())
        .extension()
        .and_then(|v| v.to_str())
        .is_some();
    if looks_like_file {
        let deleted = tracks::delete_by_path(pool, path.as_str()).await?;
        let cleanup_scope = path.parent();
        albums::delete_empty_storage_albums_in_scope(
            pool,
            cleanup_scope.as_ref().map(|p| p.as_str()),
        )
        .await?;
        Ok(deleted)
    } else {
        let deleted = tracks::delete_by_path_or_prefix(pool, path.as_str()).await?;
        albums::delete_empty_storage_albums_in_scope(pool, Some(path.as_str())).await?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{artists, connect, migrate};
    use tokio::sync::broadcast;

    async fn insert_storage_album_with_tracks(
        pool: &sqlx::SqlitePool,
        album_path: &str,
        track_paths: &[&str],
    ) -> i64 {
        let artist_id = artists::upsert_by_name(pool, "Artist", None).await.unwrap();
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

        for path in track_paths {
            tracks::upsert(
                pool,
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
                    file_size: None,
                },
            )
            .await
            .unwrap();
        }

        album_id
    }

    #[test]
    fn status_serializes_without_credentials() {
        let json = serde_json::to_string(&StorageWatchStatus::degraded("auth denied")).unwrap();
        assert!(json.contains("auth denied"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn watch_event_file_maps_to_parent_album_root() {
        let root = watch_event_scan_root("Music", "Music/Artist/Album/01.flac").unwrap();
        assert_eq!(root.as_str(), "Artist/Album");
    }

    #[test]
    fn watch_event_directory_maps_to_directory_root() {
        let root = watch_event_scan_root("Music", "Music/Artist/Album").unwrap();
        assert_eq!(root.as_str(), "Artist/Album");
    }

    #[test]
    fn unsafe_watch_event_path_maps_to_full_scan() {
        assert!(watch_event_scan_root("Music", "Music/../bad.flac").is_none());
    }

    #[test]
    fn removed_watch_event_maps_to_prune_path() {
        let event = euterpe_smb::SmbWatchEvent {
            path: "Music/Artist/Album/01.flac".into(),
            action: euterpe_smb::SmbWatchAction::Removed,
        };
        assert_eq!(
            watch_event_change("Music", &event),
            PendingWatchChange::Prune(StoragePath::parse("Artist/Album/01.flac").unwrap())
        );
    }

    #[test]
    fn renamed_new_watch_event_maps_to_rescan_parent() {
        let event = euterpe_smb::SmbWatchEvent {
            path: "Music/Artist/Album/01.flac".into(),
            action: euterpe_smb::SmbWatchAction::RenamedNew,
        };
        assert_eq!(
            watch_event_change("Music", &event),
            PendingWatchChange::Scan(Some(StoragePath::parse("Artist/Album").unwrap()))
        );
    }

    #[test]
    fn coalesce_keeps_same_album_root() {
        let root = StoragePath::parse("Artist/Album").unwrap();
        assert_eq!(
            coalesce_scan_roots(vec![Some(root.clone()), Some(root.clone())]),
            Some(root)
        );
    }

    #[test]
    fn coalesce_multiple_roots_to_full_scan() {
        assert_eq!(
            coalesce_scan_roots(vec![
                Some(StoragePath::parse("Artist/A").unwrap()),
                Some(StoragePath::parse("Artist/B").unwrap())
            ]),
            None
        );
    }

    #[tokio::test]
    async fn file_prune_deletes_only_matching_track_and_keeps_album() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let album_id = insert_storage_album_with_tracks(
            &pool,
            "Artist/Album",
            &["Artist/Album/01.flac", "Artist/Album/02.flac"],
        )
        .await;

        let deleted =
            prune_removed_watch_path(&pool, &StoragePath::parse("Artist/Album/01.flac").unwrap())
                .await
                .unwrap();

        assert_eq!(deleted, 1);
        let rows = tracks::list_by_album(&pool, album_id).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "Artist/Album/02.flac");
        assert!(albums::get_by_id(&pool, album_id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn directory_prune_deletes_children_and_keeps_sibling_prefix_album() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let album_id = insert_storage_album_with_tracks(
            &pool,
            "Artist/Album",
            &["Artist/Album/01.flac", "Artist/Album/02.flac"],
        )
        .await;
        let sibling_id =
            insert_storage_album_with_tracks(&pool, "Artist/AlbumX", &["Artist/AlbumX/01.flac"])
                .await;

        let deleted = prune_removed_watch_path(&pool, &StoragePath::parse("Artist/Album").unwrap())
            .await
            .unwrap();

        assert_eq!(deleted, 2);
        assert!(
            tracks::list_by_album(&pool, album_id)
                .await
                .unwrap()
                .is_empty()
        );
        let sibling_rows = tracks::list_by_album(&pool, sibling_id).await.unwrap();
        assert_eq!(sibling_rows.len(), 1);
        assert_eq!(sibling_rows[0].path, "Artist/AlbumX/01.flac");
    }

    #[tokio::test]
    async fn prune_of_last_track_removes_empty_storage_album() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let album_id =
            insert_storage_album_with_tracks(&pool, "Artist/Album", &["Artist/Album/01.flac"])
                .await;

        let deleted =
            prune_removed_watch_path(&pool, &StoragePath::parse("Artist/Album/01.flac").unwrap())
                .await
                .unwrap();

        assert_eq!(deleted, 1);
        assert!(albums::get_by_id(&pool, album_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn prune_only_debounce_batch_does_not_start_scan() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let album_id =
            insert_storage_album_with_tracks(&pool, "Artist/Album", &["Artist/Album/01.flac"])
                .await;
        let mut settings = crate::services::app_settings::RuntimeSettings::default();
        settings.storage.library = Some(StorageLocation::Smb {
            host: "localhost".into(),
            port: 445,
            share: "music".into(),
            path: "Music".into(),
            username: None,
            password_encrypted: None,
            workgroup: None,
        });
        let runtime = Arc::new(RwLock::new(settings));
        let (scan_events, mut scan_rx) = broadcast::channel(8);
        let (convert_job_tx, _convert_job_rx) = mpsc::channel(1);
        let deps = StorageWatchDeps {
            pool: pool.clone(),
            config: Arc::new(AppConfig::from_env().unwrap()),
            runtime,
            scan_events,
            convert_job_tx,
        };
        let watch = StorageWatchHandle::new(deps.clone());

        schedule_debounced_changes(
            &deps,
            &watch,
            false,
            None,
            vec![StoragePath::parse("Artist/Album/01.flac").unwrap()],
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        assert!(
            tracks::list_by_album(&pool, album_id)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(scan_rx.try_recv().is_err());
    }
}

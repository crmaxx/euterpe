use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::AppConfig;
use crate::db::library_scan_runs;
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
    task: Arc<RwLock<Option<JoinHandle<()>>>>,
    deps: StorageWatchDeps,
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

    pub async fn restart(&self) {
        if let Some(task) = self.task.write().await.take() {
            task.abort();
        }
        let location = self.deps.runtime.read().await.storage.library.clone();
        match location {
            Some(StorageLocation::Smb { .. }) => {
                let deps = self.deps.clone();
                let status = self.status.clone();
                *status.write().await =
                    StorageWatchStatus::reconnecting("starting SMB ChangeNotify watcher");
                let task = tokio::spawn(async move {
                    watch_loop(deps, status).await;
                });
                *self.task.write().await = Some(task);
            }
            Some(StorageLocation::Local { .. }) | None => {
                *self.status.write().await = StorageWatchStatus::disabled();
            }
        }
    }
}

async fn watch_loop(deps: StorageWatchDeps, status: Arc<RwLock<StorageWatchStatus>>) {
    let mut backoff_idx = 0usize;
    loop {
        match watch_once(&deps, &status).await {
            Ok(()) => {
                backoff_idx = 0;
                *status.write().await =
                    StorageWatchStatus::reconnecting("SMB watcher disconnected");
            }
            Err(error) => {
                let reason = error.to_string();
                *status.write().await = StorageWatchStatus::degraded(reason.clone());
                tracing::warn!(error = %reason, "SMB storage watcher degraded");
            }
        }
        let delay = BACKOFF_STEPS[backoff_idx.min(BACKOFF_STEPS.len() - 1)];
        backoff_idx = (backoff_idx + 1).min(BACKOFF_STEPS.len() - 1);
        tokio::time::sleep(delay).await;
        *status.write().await = StorageWatchStatus::reconnecting("reconnecting SMB watcher");
    }
}

async fn watch_once(
    deps: &StorageWatchDeps,
    status: &Arc<RwLock<StorageWatchStatus>>,
) -> Result<(), ApiError> {
    let location = deps.runtime.read().await.storage.library.clone();
    let Some(StorageLocation::Smb {
        host,
        port,
        share,
        path,
        username,
        password_encrypted,
        ..
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
    let credentials = euterpe_smb::SmbCredentials {
        username: username.unwrap_or_default(),
        password,
    };
    let root_path = euterpe_smb::normalize_remote_path(&location.path);
    let mut stream = euterpe_smb::SmbStorageClient::new()
        .watch_directory(&location, &credentials, true)
        .await
        .map_err(|e| ApiError::Message(format!("SMB_CHANGE_NOTIFY_FAILED: {e}")))?;
    let (scan_tx, scan_rx) = mpsc::channel(256);
    let scan_deps = deps.clone();
    let scan_task = tokio::spawn(async move {
        if let Err(error) = run_debounce_worker(scan_deps, scan_rx).await {
            tracing::warn!(error = %error, "storage watch debounce worker failed");
        }
    });
    *status.write().await = StorageWatchStatus::connected();
    while let Some(item) = stream.next().await {
        let event =
            item.map_err(|e| ApiError::Message(format!("SMB_CHANGE_NOTIFY_FAILED: {e}")))?;
        let root = watch_event_scan_root(&root_path, &event.path);
        if scan_tx.send(root).await.is_err() {
            break;
        }
    }
    drop(scan_tx);
    scan_task.abort();
    Ok(())
}

async fn run_debounce_worker(
    deps: StorageWatchDeps,
    mut rx: mpsc::Receiver<Option<StoragePath>>,
) -> Result<(), ApiError> {
    while let Some(first) = rx.recv().await {
        let mut roots = vec![first];
        let delay = tokio::time::sleep(DEBOUNCE_WINDOW);
        tokio::pin!(delay);
        loop {
            tokio::select! {
                maybe_root = rx.recv() => {
                    match maybe_root {
                        Some(root) => roots.push(root),
                        None => break,
                    }
                }
                _ = &mut delay => break,
            }
        }
        schedule_debounced_scan(&deps, coalesce_scan_roots(roots)).await?;
    }
    Ok(())
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
    let rel = strip_watch_root(library_root, event_path)?;
    if rel.is_empty() {
        return None;
    }
    let rel_path = StoragePath::parse(&rel).ok()?;
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
    path: Option<StoragePath>,
) -> Result<(), ApiError> {
    while library_scan_runs::has_running(&deps.pool).await? {
        tracing::info!("storage watch rescan pending: scan already running");
        tokio::time::sleep(PENDING_SCAN_RETRY).await;
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
    let _ = crate::services::library_scan::start_scan_storage(
        &deps.pool,
        storage,
        deps.scan_events.clone(),
        scan_cfg,
        path,
        Some(deps.convert_job_tx.clone()),
        Some(deps.runtime.clone()),
    )
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

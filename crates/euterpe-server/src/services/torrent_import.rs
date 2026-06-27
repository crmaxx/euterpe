use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::BytesMut;
use euterpe_data::DataHandle;
use futures_util::stream;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::sync::broadcast;

use crate::api::ScanProgressEvent;
use crate::config::LibraryScanConfig;
use crate::error::ApiError;
use crate::library::storage::{LibraryStorage, StoragePath};
use crate::services::library_scan;

const TORRENT_IMPORT_CHUNK_SIZE: usize = 64 * 1024;
pub type TorrentImportCancel = Arc<dyn Fn() -> bool + Send + Sync>;

pub fn safe_folder_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => out.push('_'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "torrent".into()
    } else {
        trimmed.to_string()
    }
}

pub async fn unique_library_dest(
    library_path: &Path,
    base_name: &str,
) -> Result<PathBuf, ApiError> {
    let safe = safe_folder_name(base_name);
    let mut dest = library_path.join(&safe);
    if !dest.exists() {
        return Ok(dest);
    }
    for n in 2..1000 {
        dest = library_path.join(format!("{safe} ({n})"));
        if !dest.exists() {
            return Ok(dest);
        }
    }
    Err(ApiError::Message(
        "could not allocate library folder name".into(),
    ))
}

pub async fn unique_library_dest_storage(
    storage: &dyn LibraryStorage,
    display_name: &str,
) -> Result<StoragePath, ApiError> {
    let safe = safe_folder_name(display_name);
    let mut dest = StoragePath::parse(&safe)?;
    if storage.metadata(&dest).await.is_err() {
        return Ok(dest);
    }
    for n in 2..1000 {
        dest = StoragePath::parse(format!("{safe} ({n})"))?;
        if storage.metadata(&dest).await.is_err() {
            return Ok(dest);
        }
    }
    Err(ApiError::Message(
        "could not allocate library folder name".into(),
    ))
}

pub async fn copy_dir_contents(src: &Path, dest: &Path) -> Result<(), ApiError> {
    fs::create_dir_all(dest)
        .await
        .map_err(|e| ApiError::Message(format!("mkdir {}: {e}", dest.display())))?;

    let mut read_dir = fs::read_dir(src)
        .await
        .map_err(|e| ApiError::Message(format!("read_dir {}: {e}", src.display())))?;

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?
    {
        let path = entry.path();
        let file_name = entry.file_name();
        let target = dest.join(&file_name);
        if path.is_dir() {
            Box::pin(copy_dir_contents(&path, &target)).await?;
        } else {
            fs::copy(&path, &target).await.map_err(|e| {
                ApiError::Message(format!(
                    "copy {} -> {}: {e}",
                    path.display(),
                    target.display()
                ))
            })?;
        }
    }
    Ok(())
}

pub async fn copy_local_tree_to_storage(
    source_dir: &Path,
    storage: &dyn LibraryStorage,
    dest_root: &StoragePath,
) -> Result<(), ApiError> {
    copy_local_tree_to_storage_cancellable(source_dir, storage, dest_root, Arc::new(|| false)).await
}

pub async fn copy_local_tree_to_storage_cancellable(
    source_dir: &Path,
    storage: &dyn LibraryStorage,
    dest_root: &StoragePath,
    should_cancel: TorrentImportCancel,
) -> Result<(), ApiError> {
    ensure_not_cancelled(&should_cancel)?;
    storage.create_dir_all(dest_root).await?;

    let mut read_dir = fs::read_dir(source_dir)
        .await
        .map_err(|e| ApiError::Message(format!("read_dir {}: {e}", source_dir.display())))?;
    let mut entries = Vec::new();

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?
    {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        let meta = fs::metadata(&path)
            .await
            .map_err(|e| ApiError::Message(format!("metadata {}: {e}", path.display())))?;
        entries.push((file_name, path, meta));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (file_name, path, meta) in entries {
        ensure_not_cancelled(&should_cancel)?;
        let target = dest_root.join(&file_name)?;
        if meta.is_dir() {
            Box::pin(copy_local_tree_to_storage_cancellable(
                &path,
                storage,
                &target,
                should_cancel.clone(),
            ))
            .await?;
        } else {
            let file = fs::File::open(&path)
                .await
                .map_err(|e| ApiError::Message(format!("read {}: {e}", path.display())))?;
            let stream = cancellable_file_stream(file, should_cancel.clone());
            storage.atomic_write_stream(&target, stream).await?;
        }
    }
    Ok(())
}

fn ensure_not_cancelled(should_cancel: &TorrentImportCancel) -> Result<(), ApiError> {
    if should_cancel() {
        Err(ApiError::Message("torrent import cancelled".into()))
    } else {
        Ok(())
    }
}

fn cancellable_file_stream(
    file: fs::File,
    should_cancel: TorrentImportCancel,
) -> crate::library::storage::StorageByteStream {
    Box::pin(stream::try_unfold(file, move |mut file| {
        let should_cancel = should_cancel.clone();
        async move {
            if should_cancel() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "torrent import cancelled",
                ));
            }
            let mut buf = BytesMut::zeroed(TORRENT_IMPORT_CHUNK_SIZE);
            let n = file.read(&mut buf).await?;
            if n == 0 {
                Ok(None)
            } else {
                buf.truncate(n);
                Ok(Some((buf.freeze(), file)))
            }
        }
    }))
}

pub async fn copy_to_library(
    incoming_job_dir: &Path,
    library_path: &Path,
    display_name: &str,
) -> Result<(PathBuf, String), ApiError> {
    let dest = unique_library_dest(library_path, display_name).await?;
    copy_dir_contents(incoming_job_dir, &dest).await?;
    let rel = dest
        .strip_prefix(library_path)
        .map_err(|_| ApiError::Message("library dest not under library_path".into()))?
        .to_string_lossy()
        .replace('\\', "/");
    Ok((dest, rel))
}

pub async fn copy_to_library_storage(
    source_dir: &Path,
    storage: &dyn LibraryStorage,
    display_name: &str,
) -> Result<String, ApiError> {
    copy_to_library_storage_cancellable(source_dir, storage, display_name, Arc::new(|| false)).await
}

pub async fn copy_to_library_storage_cancellable(
    source_dir: &Path,
    storage: &dyn LibraryStorage,
    display_name: &str,
    should_cancel: TorrentImportCancel,
) -> Result<String, ApiError> {
    let dest = unique_library_dest_storage(storage, display_name).await?;
    copy_local_tree_to_storage_cancellable(source_dir, storage, &dest, should_cancel).await?;
    Ok(dest.as_str().to_string())
}

pub async fn trigger_library_scan(
    data: &DataHandle,
    library_path: PathBuf,
    scan_events: broadcast::Sender<ScanProgressEvent>,
    scan_cfg: LibraryScanConfig,
    library_dest_rel: &str,
) -> Result<i64, ApiError> {
    let scan_root = library_scan::resolve_scan_root_query(&library_path, Some(library_dest_rel))?;
    library_scan::start_scan(
        data,
        library_path,
        scan_events,
        scan_cfg,
        scan_root,
        None,
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::StreamExt;

    use crate::library::storage::{LocalStorage, StorageByteStream, StorageEntry, StorageMetadata};

    #[derive(Default)]
    struct RecordingStorage {
        files: tokio::sync::Mutex<HashMap<String, Bytes>>,
        dirs: tokio::sync::Mutex<HashSet<String>>,
        stream_writes: tokio::sync::Mutex<Vec<(String, usize)>>,
        atomic_writes: tokio::sync::Mutex<Vec<String>>,
        cancel_after_first_stream: Option<Arc<AtomicBool>>,
    }

    #[async_trait]
    impl LibraryStorage for RecordingStorage {
        async fn metadata(&self, path: &StoragePath) -> Result<StorageMetadata, ApiError> {
            let key = path.as_str().to_string();
            if self.files.lock().await.contains_key(&key) || self.dirs.lock().await.contains(&key) {
                Ok(StorageMetadata {
                    kind: crate::library::storage::StorageEntryKind::File,
                    size: 0,
                    mtime: None,
                })
            } else {
                Err(ApiError::Message("not found".into()))
            }
        }

        async fn list_dir(&self, _path: &StoragePath) -> Result<Vec<StorageEntry>, ApiError> {
            Ok(Vec::new())
        }

        async fn read(&self, path: &StoragePath) -> Result<Bytes, ApiError> {
            self.files
                .lock()
                .await
                .get(path.as_str())
                .cloned()
                .ok_or_else(|| ApiError::Message("not found".into()))
        }

        async fn read_at(
            &self,
            path: &StoragePath,
            offset: u64,
            len: usize,
        ) -> Result<Bytes, ApiError> {
            let bytes = self.read(path).await?;
            let start = offset as usize;
            let end = (start + len).min(bytes.len());
            Ok(bytes.slice(start..end))
        }

        async fn read_stream(
            &self,
            _path: &StoragePath,
            _offset: u64,
            _len: Option<u64>,
        ) -> Result<StorageByteStream, ApiError> {
            Err(ApiError::Message("unused".into()))
        }

        async fn atomic_write_stream(
            &self,
            path: &StoragePath,
            mut stream: StorageByteStream,
        ) -> Result<(), ApiError> {
            let mut chunks = 0;
            let mut body = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| ApiError::Message(e.to_string()))?;
                chunks += 1;
                body.extend_from_slice(&chunk);
            }
            self.stream_writes
                .lock()
                .await
                .push((path.as_str().to_string(), chunks));
            self.files
                .lock()
                .await
                .insert(path.as_str().to_string(), Bytes::from(body));
            if let Some(cancelled) = &self.cancel_after_first_stream {
                cancelled.store(true, Ordering::SeqCst);
            }
            Ok(())
        }

        async fn atomic_write(&self, path: &StoragePath, bytes: Bytes) -> Result<(), ApiError> {
            self.atomic_writes
                .lock()
                .await
                .push(path.as_str().to_string());
            self.files
                .lock()
                .await
                .insert(path.as_str().to_string(), bytes);
            Ok(())
        }

        async fn create_dir_all(&self, path: &StoragePath) -> Result<(), ApiError> {
            self.dirs.lock().await.insert(path.as_str().to_string());
            Ok(())
        }

        async fn rename(&self, _from: &StoragePath, _to: &StoragePath) -> Result<(), ApiError> {
            Ok(())
        }

        async fn delete(&self, path: &StoragePath) -> Result<(), ApiError> {
            self.files.lock().await.remove(path.as_str());
            Ok(())
        }
    }

    #[tokio::test]
    async fn unique_library_dest_storage_appends_suffix_for_existing_album() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path());
        let existing = StoragePath::parse("Album").unwrap();
        storage.create_dir_all(&existing).await.unwrap();

        let dest = unique_library_dest_storage(&storage, "Album")
            .await
            .unwrap();

        assert_eq!(dest.as_str(), "Album (2)");
    }

    #[tokio::test]
    async fn copy_to_library_storage_copies_nested_tree() {
        let source = tempfile::tempdir().unwrap();
        let storage_root = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(storage_root.path());

        let disc_dir = source.path().join("Disc 1");
        fs::create_dir_all(&disc_dir).await.unwrap();
        fs::write(source.path().join("cover.jpg"), b"cover")
            .await
            .unwrap();
        fs::write(disc_dir.join("01.flac"), b"track").await.unwrap();

        let rel = copy_to_library_storage(source.path(), &storage, "Album")
            .await
            .unwrap();

        assert_eq!(rel, "Album");
        assert_eq!(
            storage
                .read(&StoragePath::parse("Album/cover.jpg").unwrap())
                .await
                .unwrap(),
            Bytes::from_static(b"cover")
        );
        assert_eq!(
            storage
                .read(&StoragePath::parse("Album/Disc 1/01.flac").unwrap())
                .await
                .unwrap(),
            Bytes::from_static(b"track")
        );
    }

    #[tokio::test]
    async fn copy_to_library_storage_streams_large_files_without_atomic_write_body() {
        let source = tempfile::tempdir().unwrap();
        let storage = RecordingStorage::default();
        let large = vec![7_u8; 128 * 1024];
        fs::write(source.path().join("large.flac"), &large)
            .await
            .unwrap();

        let rel = copy_to_library_storage(source.path(), &storage, "Album")
            .await
            .unwrap();

        assert_eq!(rel, "Album");
        assert_eq!(
            storage
                .read(&StoragePath::parse("Album/large.flac").unwrap())
                .await
                .unwrap(),
            Bytes::from(large)
        );
        assert!(storage.atomic_writes.lock().await.is_empty());
        let writes = storage.stream_writes.lock().await;
        assert_eq!(writes.len(), 1);
        assert!(writes[0].1 > 1, "large file should be split into chunks");
    }

    #[tokio::test]
    async fn copy_to_library_storage_cancellation_stops_later_files() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("01.flac"), b"first")
            .await
            .unwrap();
        fs::write(source.path().join("02.flac"), b"second")
            .await
            .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let storage = RecordingStorage {
            cancel_after_first_stream: Some(cancelled.clone()),
            ..Default::default()
        };

        let err = copy_to_library_storage_cancellable(
            source.path(),
            &storage,
            "Album",
            Arc::new({
                let cancelled = cancelled.clone();
                move || cancelled.load(Ordering::SeqCst)
            }),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("cancelled"));
        assert!(
            storage
                .read(&StoragePath::parse("Album/01.flac").unwrap())
                .await
                .is_ok()
        );
        assert!(
            storage
                .read(&StoragePath::parse("Album/02.flac").unwrap())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn copy_to_library_storage_cancellation_during_file_leaves_no_partial_final_file() {
        let source = tempfile::tempdir().unwrap();
        let storage_root = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(storage_root.path());
        fs::write(source.path().join("large.flac"), vec![3_u8; 128 * 1024])
            .await
            .unwrap();
        let checks = Arc::new(AtomicUsize::new(0));

        let err = copy_to_library_storage_cancellable(
            source.path(),
            &storage,
            "Album",
            Arc::new({
                let checks = checks.clone();
                move || checks.fetch_add(1, Ordering::SeqCst) > 0
            }),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("cancelled"));
        assert!(
            storage
                .read(&StoragePath::parse("Album/large.flac").unwrap())
                .await
                .is_err()
        );
    }
}

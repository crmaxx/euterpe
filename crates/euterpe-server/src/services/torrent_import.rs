use std::path::{Path, PathBuf};

use bytes::Bytes;
use sqlx::SqlitePool;
use tokio::fs;
use tokio::sync::broadcast;

use crate::api::ScanProgressEvent;
use crate::config::LibraryScanConfig;
use crate::error::ApiError;
use crate::library::storage::{LibraryStorage, StoragePath};
use crate::services::library_scan;

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
    storage.create_dir_all(dest_root).await?;

    let mut read_dir = fs::read_dir(source_dir)
        .await
        .map_err(|e| ApiError::Message(format!("read_dir {}: {e}", source_dir.display())))?;

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?
    {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        let target = dest_root.join(&file_name)?;
        let meta = fs::metadata(&path)
            .await
            .map_err(|e| ApiError::Message(format!("metadata {}: {e}", path.display())))?;
        if meta.is_dir() {
            Box::pin(copy_local_tree_to_storage(&path, storage, &target)).await?;
        } else {
            let bytes = fs::read(&path)
                .await
                .map_err(|e| ApiError::Message(format!("read {}: {e}", path.display())))?;
            storage.atomic_write(&target, Bytes::from(bytes)).await?;
        }
    }
    Ok(())
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
    let dest = unique_library_dest_storage(storage, display_name).await?;
    copy_local_tree_to_storage(source_dir, storage, &dest).await?;
    Ok(dest.as_str().to_string())
}

pub async fn trigger_library_scan(
    pool: &SqlitePool,
    library_path: PathBuf,
    scan_events: broadcast::Sender<ScanProgressEvent>,
    scan_cfg: LibraryScanConfig,
    library_dest_rel: &str,
) -> Result<i64, ApiError> {
    let scan_root = library_scan::resolve_scan_root_query(&library_path, Some(library_dest_rel))?;
    library_scan::start_scan(
        pool,
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
    use crate::library::storage::LocalStorage;

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
}

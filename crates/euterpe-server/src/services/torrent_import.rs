use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use tokio::fs;
use tokio::sync::broadcast;

use crate::api::ScanProgressEvent;
use crate::config::LibraryScanConfig;
use crate::error::ApiError;
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

pub async fn unique_library_dest(library_path: &Path, base_name: &str) -> Result<PathBuf, ApiError> {
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
    Err(ApiError::Message("could not allocate library folder name".into()))
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

pub async fn trigger_library_scan(
    pool: &SqlitePool,
    library_path: PathBuf,
    scan_events: broadcast::Sender<ScanProgressEvent>,
    scan_cfg: LibraryScanConfig,
    library_dest_rel: &str,
) -> Result<i64, ApiError> {
    let scan_root = library_scan::resolve_scan_root_query(&library_path, Some(library_dest_rel))?;
    library_scan::start_scan(pool, library_path, scan_events, scan_cfg, scan_root).await
}

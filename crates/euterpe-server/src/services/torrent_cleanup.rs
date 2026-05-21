use std::path::{Component, Path, PathBuf};

use crate::db::download_jobs;
use crate::error::ApiError;
use crate::state::AppState;

/// Remove a job's directory under `torrent-incoming` (cancel / purge).
pub async fn remove_job_incoming_dir(state: &AppState, job_id: i64) -> Result<(), ApiError> {
    let Some(incoming) = state.config.torrent_incoming_dir.as_ref() else {
        return Ok(());
    };
    let Some(payload) = download_jobs::get_payload(&state.db, job_id).await? else {
        return Ok(());
    };
    let Some(torrent) = payload.torrent else {
        return Ok(());
    };
    if torrent.save_dir_incoming.trim().is_empty() {
        return Ok(());
    }
    let dir = PathBuf::from(&torrent.save_dir_incoming);
    if !is_safe_incoming_subdir(incoming, &dir) {
        tracing::warn!(
            job_id,
            dir = %dir.display(),
            "skip torrent incoming cleanup: path outside incoming dir"
        );
        return Ok(());
    }
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => {
            tracing::debug!(job_id, dir = %dir.display(), "removed torrent incoming job dir");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(ApiError::Message(format!(
                "remove torrent dir {}: {e}",
                dir.display()
            )));
        }
    }
    Ok(())
}

fn is_safe_incoming_subdir(incoming: &Path, job_dir: &Path) -> bool {
    if !job_dir.is_absolute() {
        return false;
    }
    for component in job_dir.components() {
        if matches!(component, Component::ParentDir) {
            return false;
        }
    }
    job_dir.starts_with(incoming) && job_dir != incoming
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_dir_escape() {
        let incoming = PathBuf::from("/data/incoming");
        let evil = PathBuf::from("/data/incoming/../else");
        assert!(!is_safe_incoming_subdir(&incoming, &evil));
    }

    #[test]
    fn accepts_job_subdir() {
        let incoming = PathBuf::from("/data/incoming");
        let job = PathBuf::from("/data/incoming/42");
        assert!(is_safe_incoming_subdir(&incoming, &job));
    }
}

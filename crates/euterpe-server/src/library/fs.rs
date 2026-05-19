use std::path::Path;

fn format_mtime(t: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = t.into();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn file_mtime_sync(path: &Path) -> Option<String> {
    file_stat_sync(path).0
}

/// `(mtime, size_bytes)` from a single `metadata()` call.
pub fn file_stat_sync(path: &Path) -> (Option<String>, u64) {
    match std::fs::metadata(path) {
        Ok(meta) => {
            let mtime = meta.modified().ok().map(format_mtime);
            (mtime, meta.len())
        }
        Err(_) => (None, 0),
    }
}

pub async fn file_mtime(path: &Path) -> Option<String> {
    tokio::fs::metadata(path)
        .await
        .ok()?
        .modified()
        .ok()
        .map(format_mtime)
}

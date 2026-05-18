use std::path::Path;

fn format_mtime(t: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = t.into();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn file_mtime_sync(path: &Path) -> Option<String> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()
        .map(format_mtime)
}

pub async fn file_mtime(path: &Path) -> Option<String> {
    tokio::fs::metadata(path)
        .await
        .ok()?
        .modified()
        .ok()
        .map(format_mtime)
}

#![allow(dead_code)]

pub mod albums;
pub mod artists;
pub mod convert_jobs;
pub mod cue_jobs;
pub mod download_jobs;
pub mod favorites;
pub mod integrations;
pub mod library_scan_runs;
pub mod qobuz_accounts;
pub mod settings;
pub mod sync_runs;
pub mod tracks;

use sqlx::SqlitePool;
use std::path::PathBuf;

use crate::error::ApiError;
use euterpe_data::{DataHandle, connect_database, migrations};

/// SQLite creates the DB file but not parent directories (SQLITE_CANTOPEN otherwise).
fn ensure_db_parent_dir(database_url: &str) -> Result<(), ApiError> {
    let Some(path) = sqlite_file_path(database_url) else {
        return Ok(());
    };
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            ApiError::Config(format!(
                "cannot create database directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    Ok(())
}

fn sqlite_file_path(database_url: &str) -> Option<PathBuf> {
    if database_url.contains(":memory:") {
        return None;
    }
    let rest = database_url.strip_prefix("sqlite:")?;
    let path_part = rest.split('?').next()?.trim();
    if path_part.is_empty() || path_part == ":memory:" {
        return None;
    }
    Some(PathBuf::from(path_part))
}

pub async fn connect(database_url: &str) -> Result<SqlitePool, ApiError> {
    Ok(connect_database(database_url).await?.sqlx_pool())
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    migrations::migrate(&handle).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_file_path_parses_docker_style_url() {
        let p = sqlite_file_path("sqlite:/data/library.db?mode=rwc").unwrap();
        assert_eq!(p.to_str(), Some("/data/library.db"));
    }

    #[test]
    fn ensure_db_parent_dir_creates_nested_path() {
        let base = std::env::temp_dir().join("euterpe-db-test");
        let db = base.join("nested/library.db");
        let _ = std::fs::remove_dir_all(&base);
        let url = format!("sqlite:{}?mode=rwc", db.display());
        ensure_db_parent_dir(&url).unwrap();
        assert!(base.join("nested").is_dir());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn migrations_apply_on_memory_db() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let data = DataHandle::from_sqlite_pool(pool);
        crate::test_db::settings::set(&data.sqlx_pool(), "smoke", "ok")
            .await
            .unwrap();
        assert_eq!(
            crate::test_db::settings::get(&data.sqlx_pool(), "smoke")
                .await
                .unwrap()
                .as_deref(),
            Some("ok")
        );
    }
}

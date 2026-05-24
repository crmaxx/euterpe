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
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::PathBuf;
use std::str::FromStr;

use crate::error::ApiError;

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
    ensure_db_parent_dir(database_url)?;
    let url = database_url.strip_prefix("sqlite:").unwrap_or(database_url);
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await?;

    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), ApiError> {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .map_err(|e| ApiError::Message(format!("migration failed: {e}")))?;
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
        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let names: Vec<_> = tables.into_iter().map(|(n,)| n).collect();
        assert!(names.contains(&"settings".to_string()));
        assert!(names.contains(&"qobuz_favorites".to_string()));
        assert!(names.contains(&"qobuz_sync_runs".to_string()));
        assert!(names.contains(&"download_jobs".to_string()));
        assert!(names.contains(&"artists".to_string()));
        assert!(names.contains(&"albums".to_string()));
        assert!(names.contains(&"tracks".to_string()));
        assert!(names.contains(&"library_scan_runs".to_string()));
        assert!(names.contains(&"qobuz_accounts".to_string()));
        assert!(names.contains(&"qobuz_oauth_states".to_string()));
        assert!(names.contains(&"integrations".to_string()));

        let cols: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM pragma_table_info('tracks') ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();
        let col_names: Vec<_> = cols.into_iter().map(|(n,)| n).collect();
        assert!(
            col_names.contains(&"file_size".to_string()),
            "tracks.file_size column missing after migrations"
        );
    }
}

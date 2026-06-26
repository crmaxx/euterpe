use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::path::PathBuf;
use std::str::FromStr;
use welds::connections::sqlite::SqliteClient;

use crate::error::{DataError, Result};

#[derive(Clone)]
pub struct DataHandle {
    client: SqliteClient,
}

impl DataHandle {
    pub async fn connect(database_url: &str) -> Result<Self> {
        ensure_db_parent_dir(database_url)?;
        let url = database_url.strip_prefix("sqlite:").unwrap_or(database_url);
        if url.trim().is_empty() {
            return Err(DataError::Config("empty SQLite database URL".to_string()));
        }

        let options = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        Ok(Self {
            client: SqliteClient::from(pool),
        })
    }

    pub fn client(&self) -> &SqliteClient {
        &self.client
    }
}

pub async fn connect_database(database_url: &str) -> Result<DataHandle> {
    DataHandle::connect(database_url).await
}

fn ensure_db_parent_dir(database_url: &str) -> Result<()> {
    let Some(path) = sqlite_file_path(database_url) else {
        return Ok(());
    };
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            DataError::Config(format!(
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

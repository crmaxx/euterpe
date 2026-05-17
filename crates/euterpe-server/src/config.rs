use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::crypto::MasterKey;
use crate::error::ApiError;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind: SocketAddr,
    pub database_url: String,
    pub admin_password: Option<String>,
    pub master_key: Option<MasterKey>,
    pub qobuz_user_id: Option<u64>,
    pub qobuz_auth_token: Option<String>,
    pub library_path: PathBuf,
    pub download_concurrency: usize,
    /// Verbose HTTP + Qobuz API debug logs (`EUTERPE_DEV=true`).
    pub dev_verbose: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ApiError> {
        let bind = env::var("EUTERPE_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8080".into())
            .parse::<SocketAddr>()
            .map_err(|e| ApiError::Config(format!("invalid EUTERPE_BIND: {e}")))?;

        let database_url = env::var("EUTERPE_DATABASE_URL").unwrap_or_else(|_| {
            let mut path = PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into()));
            path.push(".local/share/euterpe/library.db");
            // Parent dirs are created in db::connect (SQLite does not mkdir -p).
            format!("sqlite:{}?mode=rwc", path.display())
        });

        let admin_password = env::var("EUTERPE_ADMIN_PASSWORD").ok();

        let master_key = match env::var("EUTERPE_MASTER_KEY") {
            Ok(v) if !v.is_empty() => Some(MasterKey::parse(&v)?),
            _ => None,
        };

        let qobuz_user_id = env::var("EUTERPE_QOBUZ_USER_ID")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<u64>()
                    .map_err(|e| ApiError::Config(format!("invalid EUTERPE_QOBUZ_USER_ID: {e}")))
            })
            .transpose()?;

        let qobuz_auth_token = env::var("EUTERPE_QOBUZ_AUTH_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());

        let library_path = PathBuf::from(
            env::var("EUTERPE_LIBRARY_PATH").unwrap_or_else(|_| "/music".into()),
        );

        let download_concurrency = env::var("EUTERPE_DOWNLOAD_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        let dev_verbose = env::var("EUTERPE_DEV")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        Ok(Self {
            bind,
            database_url,
            admin_password,
            master_key,
            qobuz_user_id,
            qobuz_auth_token,
            library_path,
            download_concurrency,
            dev_verbose,
        })
    }

    pub fn ensure_library_root(&self) -> Result<(), ApiError> {
        std::fs::create_dir_all(&self.library_path).map_err(|e| {
            ApiError::Config(format!(
                "cannot create EUTERPE_LIBRARY_PATH {}: {e}",
                self.library_path.display()
            ))
        })
    }
}

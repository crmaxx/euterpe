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

        Ok(Self {
            bind,
            database_url,
            admin_password,
            master_key,
            qobuz_user_id,
            qobuz_auth_token,
        })
    }
}

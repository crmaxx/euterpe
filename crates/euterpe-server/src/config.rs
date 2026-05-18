use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use crate::crypto::MasterKey;
use crate::error::ApiError;

/// Load `.env` from the process current working directory if the file exists.
///
/// Variables already set in the environment are **not** overridden (Docker/CI env wins).
pub fn load_dotenv() {
    if let Err(e) = dotenvy::dotenv() {
        match e {
            dotenvy::Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {}
            other => eprintln!("warning: could not load .env: {other}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind: SocketAddr,
    pub database_url: String,
    pub admin_password: Option<String>,
    pub master_key: Option<MasterKey>,
    /// Public origin for OAuth redirect (e.g. `http://127.0.0.1:8080`). Derived from bind if unset.
    pub public_base_url: String,
    pub oauth_state_ttl: Duration,
    /// Override Qobuz API base (tests / custom proxy).
    pub qobuz_api_base: Option<String>,
    /// Override play.qobuz.com base (bundle fetch).
    pub qobuz_play_base: Option<String>,
    pub library_path: PathBuf,
    pub download_concurrency: usize,
    /// Verbose HTTP + Qobuz API debug logs (`EUTERPE_DEV=true`).
    pub dev_verbose: bool,
    /// Static SPA root (`index.html` + assets). Empty = disabled.
    pub static_dir: PathBuf,
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
            format!("sqlite:{}?mode=rwc", path.display())
        });

        let admin_password = env::var("EUTERPE_ADMIN_PASSWORD").ok();

        let master_key = match env::var("EUTERPE_MASTER_KEY") {
            Ok(v) if !v.is_empty() => Some(MasterKey::parse(&v)?),
            _ => None,
        };

        let public_base_url = env::var("EUTERPE_PUBLIC_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default_public_base_url(bind));

        let oauth_state_ttl_secs = env::var("EUTERPE_OAUTH_STATE_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(600);

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

        let static_dir = PathBuf::from(
            env::var("EUTERPE_STATIC_DIR").unwrap_or_else(|_| "frontend/dist".into()),
        );

        let qobuz_api_base = env::var("EUTERPE_QOBUZ_API_BASE")
            .ok()
            .filter(|s| !s.is_empty());
        let qobuz_play_base = env::var("EUTERPE_QOBUZ_PLAY_BASE")
            .ok()
            .filter(|s| !s.is_empty());

        Ok(Self {
            bind,
            database_url,
            admin_password,
            master_key,
            public_base_url: public_base_url.trim_end_matches('/').to_string(),
            oauth_state_ttl: Duration::from_secs(oauth_state_ttl_secs),
            qobuz_api_base,
            qobuz_play_base,
            library_path,
            download_concurrency,
            dev_verbose,
            static_dir,
        })
    }

    pub fn oauth_callback_url(&self) -> String {
        format!("{}/api/v1/qobuz/oauth/callback", self.public_base_url)
    }

    pub fn qobuz_api_base(&self) -> &str {
        self.qobuz_api_base
            .as_deref()
            .unwrap_or(euterpe_qobuz::oauth::default_api_base())
    }

    pub fn qobuz_play_base(&self) -> &str {
        self.qobuz_play_base
            .as_deref()
            .unwrap_or(euterpe_qobuz::oauth::default_play_base())
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn dotenv_from_path_loads_variables() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(&path, "EUTERPE_BIND=127.0.0.1:7777\n").expect("write .env");
        std::env::remove_var("EUTERPE_BIND");
        dotenvy::from_path(&path).expect("from_path");
        assert_eq!(
            std::env::var("EUTERPE_BIND").expect("EUTERPE_BIND"),
            "127.0.0.1:7777"
        );
        std::env::remove_var("EUTERPE_BIND");
    }

    #[test]
    fn dotenv_from_path_does_not_override_existing_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(&path, "EUTERPE_BIND=127.0.0.1:7777\n").expect("write .env");
        std::env::set_var("EUTERPE_BIND", "127.0.0.1:9999");
        dotenvy::from_path(&path).expect("from_path");
        assert_eq!(
            std::env::var("EUTERPE_BIND").expect("EUTERPE_BIND"),
            "127.0.0.1:9999"
        );
        std::env::remove_var("EUTERPE_BIND");
    }
}

fn default_public_base_url(bind: SocketAddr) -> String {
    let host = bind.ip().to_string();
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host
    };
    format!("http://{host}:{}", bind.port())
}

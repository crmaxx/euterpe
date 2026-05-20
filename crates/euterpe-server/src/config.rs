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

/// Parallel library scan: enumerate + process pools, bounded queues (FP-9).
#[derive(Debug, Clone)]
pub struct LibraryScanConfig {
    /// `enum_workers + process_workers` must be `<=` this value (`EUTERPE_LIBRARY_SCAN_WORKER_TOTAL`, default 10, clamped 2..32).
    pub worker_total: usize,
    /// Enumerate-only workers (`EUTERPE_LIBRARY_SCAN_ENUM_WORKERS`, default 5).
    pub enum_workers: usize,
    /// Process (tags/hash) workers (`EUTERPE_LIBRARY_SCAN_PROCESS_WORKERS`, default 5).
    pub process_workers: usize,
    /// Seed `dir_queue` with subdirs at this depth from library root (default 1).
    pub seed_depth: u32,
    /// Bounded `index_queue` capacity for backpressure (default 512).
    pub index_queue_capacity: usize,
    /// Bounded path queue between enumerate and process (default 2048).
    pub path_queue_capacity: usize,
    /// Verbose scan worker logs at `debug` level when `EUTERPE_DEBUG=true` (set on `AppConfig`).
    pub debug: bool,
}

impl Default for LibraryScanConfig {
    fn default() -> Self {
        Self {
            worker_total: 10,
            enum_workers: 5,
            process_workers: 5,
            seed_depth: 1,
            index_queue_capacity: 512,
            path_queue_capacity: 2048,
            debug: false,
        }
    }
}

impl LibraryScanConfig {
    pub fn from_env() -> Result<Self, ApiError> {
        let worker_total = env::var("EUTERPE_LIBRARY_SCAN_WORKER_TOTAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10)
            .clamp(2, 32);
        let enum_workers = env::var("EUTERPE_LIBRARY_SCAN_ENUM_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);
        let process_workers = env::var("EUTERPE_LIBRARY_SCAN_PROCESS_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        if enum_workers < 1 || process_workers < 1 {
            return Err(ApiError::Config(
                "EUTERPE_LIBRARY_SCAN_ENUM_WORKERS and EUTERPE_LIBRARY_SCAN_PROCESS_WORKERS must be >= 1"
                    .into(),
            ));
        }
        if enum_workers > worker_total || process_workers > worker_total {
            return Err(ApiError::Config(format!(
                "enum workers ({enum_workers}) and process workers ({process_workers}) must each be <= EUTERPE_LIBRARY_SCAN_WORKER_TOTAL ({worker_total})"
            )));
        }
        if enum_workers + process_workers > worker_total {
            return Err(ApiError::Config(format!(
                "EUTERPE_LIBRARY_SCAN_ENUM_WORKERS ({enum_workers}) + EUTERPE_LIBRARY_SCAN_PROCESS_WORKERS ({process_workers}) must be <= EUTERPE_LIBRARY_SCAN_WORKER_TOTAL ({worker_total})"
            )));
        }

        let seed_depth = env::var("EUTERPE_LIBRARY_SCAN_SEED_DEPTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let index_queue_capacity = env::var("EUTERPE_LIBRARY_SCAN_INDEX_QUEUE")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(512);
        let path_queue_capacity = env::var("EUTERPE_LIBRARY_SCAN_PATH_QUEUE")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(2048);
        Ok(Self {
            worker_total,
            enum_workers,
            process_workers,
            seed_depth,
            index_queue_capacity,
            path_queue_capacity,
            debug: false,
        })
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
    /// Separate directory for torrent downloads (not under library_path).
    pub torrent_incoming_dir: Option<PathBuf>,
    pub torrent_max_active: usize,
    /// UPnP port mapping for the BitTorrent listen port (librqbit). Default on.
    pub torrent_enable_upnp: bool,
    pub download_concurrency: usize,
    pub library_scan: LibraryScanConfig,
    /// Verbose HTTP, Qobuz API, library scan, and download worker logs (`EUTERPE_DEBUG=true`).
    pub debug: bool,
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

        let torrent_incoming_dir = env::var("EUTERPE_TORRENT_INCOMING_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        let torrent_max_active = env::var("EUTERPE_TORRENT_MAX_ACTIVE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2)
            .max(1);

        let torrent_enable_upnp = env::var("EUTERPE_TORRENT_UPNP")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(true);

        let download_concurrency = env::var("EUTERPE_DOWNLOAD_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        let debug = env::var("EUTERPE_DEBUG")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let mut library_scan = LibraryScanConfig::from_env()?;
        library_scan.debug = debug;

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
            torrent_incoming_dir,
            torrent_max_active,
            torrent_enable_upnp,
            download_concurrency,
            library_scan,
            debug,
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

    pub fn ensure_torrent_incoming_dir(&self) -> Result<(), ApiError> {
        if let Some(ref dir) = self.torrent_incoming_dir {
            std::fs::create_dir_all(dir).map_err(|e| {
                ApiError::Config(format!(
                    "cannot create EUTERPE_TORRENT_INCOMING_DIR {}: {e}",
                    dir.display()
                ))
            })?;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{ApiError, LibraryScanConfig};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn library_scan_rejects_enum_plus_process_over_total() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("EUTERPE_LIBRARY_SCAN_WORKER_TOTAL", "4");
        std::env::set_var("EUTERPE_LIBRARY_SCAN_ENUM_WORKERS", "3");
        std::env::set_var("EUTERPE_LIBRARY_SCAN_PROCESS_WORKERS", "3");
        let err = LibraryScanConfig::from_env().expect_err("expected config error");
        assert!(matches!(err, ApiError::Config(_)));
        std::env::remove_var("EUTERPE_LIBRARY_SCAN_WORKER_TOTAL");
        std::env::remove_var("EUTERPE_LIBRARY_SCAN_ENUM_WORKERS");
        std::env::remove_var("EUTERPE_LIBRARY_SCAN_PROCESS_WORKERS");
    }

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

use std::sync::Arc;

use euterpe_data::DataHandle;
use euterpe_qobuz::QobuzApi;
use euterpe_torrent::TorrentEngine;
use reqwest::Client;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

use crate::api::{ConvertProgressEvent, JobProgressEvent, ScanProgressEvent};
use crate::config::AppConfig;
use crate::credentials::{self, QobuzCredentials};
use crate::crypto::MasterKey;
use crate::error::ApiError;
use crate::library::storage::LibraryStorage;
use crate::services::app_settings::{self, RuntimeSettingsHandle, StorageLocation};
use crate::services::torrent_staging::TorrentStaging;

type LibraryStorageCache = Option<(StorageLocation, Arc<dyn LibraryStorage>)>;

/// Channels created at startup and passed into [`AppState::new`].
#[derive(Clone)]
pub struct AppChannels {
    pub job_tx: mpsc::Sender<i64>,
    pub convert_job_tx: mpsc::Sender<i64>,
    pub events: broadcast::Sender<JobProgressEvent>,
    pub scan_events: broadcast::Sender<ScanProgressEvent>,
    pub convert_events: broadcast::Sender<ConvertProgressEvent>,
}

#[derive(Clone)]
pub struct AppState {
    pub data: DataHandle,
    pub config: Arc<AppConfig>,
    pub http: Client,
    pub qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>>,
    pub job_tx: mpsc::Sender<i64>,
    pub convert_job_tx: mpsc::Sender<i64>,
    pub events: broadcast::Sender<JobProgressEvent>,
    pub scan_events: broadcast::Sender<ScanProgressEvent>,
    pub convert_events: broadcast::Sender<ConvertProgressEvent>,
    pub runtime: RuntimeSettingsHandle,
    pub storage_watch: crate::services::storage_watch::StorageWatchHandle,
    pub qobuz_scheduled_sync: crate::services::qobuz_scheduled_sync::QobuzScheduledSyncHandle,
    pub hawk: Option<Arc<euterpe_hawk::Hawk>>,
    pub torrent: Option<Arc<dyn TorrentEngine>>,
    pub torrent_staging: Arc<TorrentStaging>,
    /// Cached [`library_storage`] handle; cleared when storage settings change.
    #[doc(hidden)]
    pub library_storage_cache: Arc<tokio::sync::Mutex<LibraryStorageCache>>,
}

impl AppState {
    pub async fn new(
        config: AppConfig,
        data: DataHandle,
        channels: AppChannels,
        hawk: Option<Arc<euterpe_hawk::Hawk>>,
    ) -> Result<Self, ApiError> {
        let config = Arc::new(config);
        let runtime = Arc::new(RwLock::new(
            app_settings::load_runtime_settings(&data, &config).await,
        ));
        let qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> = if let Some(creds) =
            credentials::load_active(&config, &data).await?
        {
            match credentials::build_client(&creds, &config).await {
                Ok(client) => Arc::new(Mutex::new(Box::new(client))),
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "Qobuz account is configured but reconnect failed; starting with Qobuz integration degraded"
                    );
                    Arc::new(Mutex::new(Box::new(NoopQobuz)))
                }
            }
        } else {
            Arc::new(Mutex::new(Box::new(NoopQobuz)))
        };

        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| ApiError::Config(e.to_string()))?;

        let torrent = if let Some(ref dir) = config.torrent_incoming_dir {
            let settings = crate::services::torrent_settings::load(&data).await?;
            let mut session_settings =
                crate::services::torrent_settings::to_session_settings(&settings);
            session_settings.enable_upnp_port_forwarding = config.torrent_enable_upnp;
            let engine =
                euterpe_torrent::LibrqbitEngine::new(euterpe_torrent::TorrentEngineConfig {
                    incoming_dir: dir.clone(),
                    session_settings,
                })
                .await
                .map_err(|e| ApiError::Message(e.to_string()))?;
            Some(Arc::new(engine) as Arc<dyn TorrentEngine>)
        } else {
            None
        };

        let storage_watch = crate::services::storage_watch::StorageWatchHandle::new(
            crate::services::storage_watch::StorageWatchDeps {
                data: data.clone(),
                config: config.clone(),
                runtime: runtime.clone(),
                scan_events: channels.scan_events.clone(),
                convert_job_tx: channels.convert_job_tx.clone(),
            },
        );
        let qobuz_scheduled_sync =
            crate::services::qobuz_scheduled_sync::QobuzScheduledSyncHandle::new(
                crate::services::qobuz_scheduled_sync::QobuzScheduledSyncDeps {
                    data: data.clone(),
                    qobuz: qobuz.clone(),
                    runtime: runtime.clone(),
                    job_tx: channels.job_tx.clone(),
                },
            );

        Ok(Self {
            data,
            config,
            http,
            qobuz,
            job_tx: channels.job_tx,
            convert_job_tx: channels.convert_job_tx,
            events: channels.events,
            scan_events: channels.scan_events,
            convert_events: channels.convert_events,
            runtime,
            storage_watch,
            qobuz_scheduled_sync,
            hawk,
            torrent,
            torrent_staging: Arc::new(TorrentStaging::new()),
            library_storage_cache: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    pub async fn invalidate_library_storage_cache(&self) {
        *self.library_storage_cache.lock().await = None;
    }

    pub async fn require_credentials(&self) -> Result<QobuzCredentials, ApiError> {
        credentials::load_active(&self.config, &self.data)
            .await?
            .ok_or_else(|| {
                ApiError::Message("Qobuz not connected — complete OAuth in Settings".into())
            })
    }

    pub async fn reload_qobuz_from_db(&self) -> Result<(), ApiError> {
        let new_client: Box<dyn QobuzApi + Send + Sync> =
            if let Some(creds) = credentials::load_active(&self.config, &self.data).await? {
                Box::new(credentials::build_client(&creds, &self.config).await?)
            } else {
                Box::new(NoopQobuz)
            };
        *self.qobuz.lock().await = new_client;
        Ok(())
    }

    pub fn master_key(&self) -> Result<&MasterKey, ApiError> {
        self.config.master_key.as_ref().ok_or_else(|| {
            ApiError::Message("EUTERPE_MASTER_KEY is required for this operation".into())
        })
    }

    pub async fn require_local_library_path(&self) -> Result<std::path::PathBuf, ApiError> {
        app_settings::require_local_library_path(&self.runtime).await
    }

    pub async fn library_storage(
        &self,
    ) -> Result<std::sync::Arc<dyn crate::library::storage::LibraryStorage>, ApiError> {
        let storage = self.runtime.read().await.storage.library.clone();
        let location = storage.ok_or_else(|| {
            ApiError::Message(
                "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
            )
        })?;
        let mut cache = self.library_storage_cache.lock().await;
        if let Some((cached_location, cached_storage)) = cache.as_ref()
            && cached_location == &location
        {
            return Ok(cached_storage.clone());
        }
        let built = crate::library::storage::storage_from_location(
            &location,
            self.config.master_key.as_ref(),
        )?;
        *cache = Some((location, built.clone()));
        Ok(built)
    }
}

struct NoopQobuz;

#[async_trait::async_trait]
impl QobuzApi for NoopQobuz {
    async fn favorites_albums(
        &self,
        _page: euterpe_qobuz::PageRequest,
    ) -> Result<euterpe_qobuz::Page<euterpe_qobuz::AlbumSummary>, euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn favorites_all_albums(
        &self,
    ) -> Result<Vec<euterpe_qobuz::AlbumSummary>, euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn favorites_album_api_id_for_catalog(
        &self,
        _catalog_id: u64,
    ) -> Result<Option<String>, euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn favorite_add_albums(&self, _ids: &[u64]) -> Result<(), euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn favorite_remove_albums(&self, _ids: &[u64]) -> Result<(), euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn track_stream_url(
        &mut self,
        _track_id: u64,
        _quality: euterpe_qobuz::Quality,
    ) -> Result<euterpe_qobuz::StreamUrl, euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn album(
        &self,
        _album_id: u64,
    ) -> Result<euterpe_qobuz::AlbumDetail, euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn album_ref(
        &self,
        _album_id: &str,
    ) -> Result<euterpe_qobuz::AlbumDetail, euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn album_search(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<euterpe_qobuz::AlbumSummary>, euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }

    async fn artist_albums(
        &self,
        _artist_id: u64,
    ) -> Result<Vec<euterpe_qobuz::AlbumSummary>, euterpe_qobuz::QobuzError> {
        Err(euterpe_qobuz::QobuzError::Config(
            "qobuz not configured".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euterpe_data::connect_database;
    use euterpe_data::migrations;
    use euterpe_data::repositories::qobuz as qobuz_accounts;
    use euterpe_data::repositories::settings::{self, KEY_QOBUZ_ACTIVE_ACCOUNT_ID};

    fn test_config() -> AppConfig {
        let master_key = MasterKey::parse(&hex::encode([1u8; 32])).unwrap();
        AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: Some(master_key),
            public_base_url: "http://127.0.0.1:0".into(),
            oauth_state_ttl: std::time::Duration::from_secs(600),
            qobuz_api_base: None,
            qobuz_play_base: Some("http://127.0.0.1:9".into()),
            library_path: std::env::temp_dir().join("euterpe-state-test"),
            torrent_incoming_dir: None,
            torrent_max_active: 2,
            torrent_enable_upnp: false,
            download_concurrency: 2,
            library_scan: crate::config::LibraryScanConfig::default(),
            debug: false,
            static_dir: std::path::PathBuf::new(),
        }
    }

    fn test_channels() -> AppChannels {
        let (job_tx, _) = mpsc::channel(1);
        let (convert_job_tx, _) = mpsc::channel(1);
        let (events, _) = broadcast::channel(1);
        let (scan_events, _) = broadcast::channel(1);
        let (convert_events, _) = broadcast::channel(1);
        AppChannels {
            job_tx,
            convert_job_tx,
            events,
            scan_events,
            convert_events,
        }
    }

    #[tokio::test]
    async fn startup_uses_noop_qobuz_when_saved_account_cannot_reconnect() {
        let config = test_config();
        let data = connect_database(&config.database_url).await.unwrap();
        migrations::migrate(&data).await.unwrap();
        let master_key = config.master_key.as_ref().unwrap();
        let encrypted_token = master_key.encrypt("saved-token").unwrap();
        let account_id = qobuz_accounts::upsert_after_oauth(
            &data,
            99,
            &encrypted_token,
            Some("Saved User"),
            Some("Studio"),
            chrono::Utc::now(),
            None,
        )
        .await
        .unwrap();
        settings::set(&data, KEY_QOBUZ_ACTIVE_ACCOUNT_ID, &account_id.to_string())
            .await
            .unwrap();

        let state = AppState::new(config, data, test_channels(), None)
            .await
            .unwrap();

        let err = state
            .qobuz
            .lock()
            .await
            .favorites_all_albums()
            .await
            .unwrap_err();
        assert!(matches!(err, euterpe_qobuz::QobuzError::Config(_)));
    }
}

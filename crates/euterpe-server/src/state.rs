use std::sync::Arc;

use euterpe_qobuz::QobuzApi;
use reqwest::Client;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::api::{JobProgressEvent, ScanProgressEvent};
use crate::config::AppConfig;
use crate::credentials::{self, QobuzCredentials};
use crate::crypto::MasterKey;
use crate::error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<AppConfig>,
    pub http: Client,
    pub qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>>,
    pub job_tx: mpsc::Sender<i64>,
    pub events: broadcast::Sender<JobProgressEvent>,
    pub scan_events: broadcast::Sender<ScanProgressEvent>,
}

impl AppState {
    pub async fn new(
        config: AppConfig,
        db: SqlitePool,
        job_tx: mpsc::Sender<i64>,
        events: broadcast::Sender<JobProgressEvent>,
        scan_events: broadcast::Sender<ScanProgressEvent>,
    ) -> Result<Self, ApiError> {
        let config = Arc::new(config);
        let qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> = if let Some(creds) =
            credentials::load_active(&config, &db).await?
        {
            let client = credentials::build_client(&creds, &config).await?;
            Arc::new(Mutex::new(Box::new(client)))
        } else {
            Arc::new(Mutex::new(Box::new(NoopQobuz)))
        };

        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| ApiError::Config(e.to_string()))?;

        Ok(Self {
            db,
            config,
            http,
            qobuz,
            job_tx,
            events,
            scan_events,
        })
    }

    pub async fn require_credentials(&self) -> Result<QobuzCredentials, ApiError> {
        credentials::load_active(&self.config, &self.db)
            .await?
            .ok_or_else(|| {
                ApiError::Message(
                    "Qobuz not connected — complete OAuth in Settings".into(),
                )
            })
    }

    pub async fn reload_qobuz_from_db(&self) -> Result<(), ApiError> {
        let new_client: Box<dyn QobuzApi + Send + Sync> = if let Some(creds) =
            credentials::load_active(&self.config, &self.db).await?
        {
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
}

struct NoopQobuz;

#[async_trait::async_trait]
impl QobuzApi for NoopQobuz {
    async fn favorites_albums(
        &self,
        _page: euterpe_qobuz::PageRequest,
    ) -> Result<euterpe_qobuz::Page<euterpe_qobuz::AlbumSummary>, euterpe_qobuz::QobuzError>
    {
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

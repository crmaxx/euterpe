use std::sync::Arc;

use euterpe_qobuz::QobuzApi;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::api::JobProgressEvent;
use crate::config::AppConfig;
use crate::credentials::{self, QobuzCredentials};
use crate::crypto::MasterKey;
use crate::error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<AppConfig>,
    pub qobuz: Arc<Mutex<dyn QobuzApi>>,
    pub job_tx: mpsc::Sender<i64>,
    pub events: broadcast::Sender<JobProgressEvent>,
}

impl AppState {
    pub async fn new(
        config: AppConfig,
        db: SqlitePool,
        job_tx: mpsc::Sender<i64>,
        events: broadcast::Sender<JobProgressEvent>,
    ) -> Result<Self, ApiError> {
        let config = Arc::new(config);
        let qobuz: Arc<Mutex<dyn QobuzApi>> = if let Some(creds) =
            credentials::load_from_env_or_db(&config, &db).await?
        {
            let client = credentials::build_client(&creds).await?;
            Arc::new(Mutex::new(client))
        } else {
            Arc::new(Mutex::new(NoopQobuz))
        };

        Ok(Self {
            db,
            config,
            qobuz,
            job_tx,
            events,
        })
    }

    pub async fn require_credentials(&self) -> Result<QobuzCredentials, ApiError> {
        credentials::load_from_env_or_db(&self.config, &self.db)
            .await?
            .ok_or_else(|| {
                ApiError::Message(
                    "Qobuz credentials not configured (env or settings)".into(),
                )
            })
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

use std::sync::Arc;

use euterpe_qobuz::{
    AlbumDetail, AlbumSummary, ArtistRef, Page, PageRequest, QobuzApi, QobuzError, Quality,
    StreamUrl, TrackSummary,
};
use euterpe_server::AppState;
use tokio::sync::{Mutex, broadcast, mpsc};

#[path = "qobuz_account.rs"]
mod qobuz_account;

pub struct DownloadMockQobuz {
    pub album: AlbumDetail,
    pub stream_url: String,
}

impl DownloadMockQobuz {
    pub fn new() -> Self {
        Self {
            album: AlbumDetail {
                summary: AlbumSummary {
                    id: 99,
                    qobuz_id: None,
                    title: "Album".into(),
                    artist: Some(ArtistRef {
                        id: 1,
                        name: "Artist".into(),
                    }),
                    artists: None,
                    image: None,
                    release_date_original: None,
                    hires: None,
                    album_ref: None,
                    slug: None,
                    list_id: None,
                    product_id: None,
                    genre: None,
                    label: None,
                },
                tracks: Some(euterpe_qobuz::AlbumTracks {
                    items: vec![TrackSummary {
                        id: 1,
                        title: "One".into(),
                        track_number: Some(1),
                        duration: None,
                        performer: None,
                        hires_streamable: None,
                        media_number: None,
                        genre: None,
                        isrc: None,
                        composer: None,
                    }],
                }),
                description: None,
            },
            stream_url: "http://127.0.0.1:9/cdn".into(),
        }
    }
}

#[async_trait::async_trait]
impl QobuzApi for DownloadMockQobuz {
    async fn favorites_albums(&self, _page: PageRequest) -> Result<Page<AlbumSummary>, QobuzError> {
        unimplemented!()
    }

    async fn favorites_all_albums(&self) -> Result<Vec<AlbumSummary>, QobuzError> {
        Ok(vec![])
    }

    async fn favorites_album_api_id_for_catalog(
        &self,
        _catalog_id: u64,
    ) -> Result<Option<String>, QobuzError> {
        Ok(None)
    }

    async fn favorite_add_albums(&self, _ids: &[u64]) -> Result<(), QobuzError> {
        Ok(())
    }

    async fn favorite_remove_albums(&self, _ids: &[u64]) -> Result<(), QobuzError> {
        Ok(())
    }

    async fn track_stream_url(
        &mut self,
        _track_id: u64,
        _quality: Quality,
    ) -> Result<StreamUrl, QobuzError> {
        Ok(StreamUrl {
            url: Some(self.stream_url.clone()),
            format_id: Some(6),
            sampling_rate: None,
            bit_depth: None,
            restrictions: None,
        })
    }

    async fn album(&self, _album_id: u64) -> Result<AlbumDetail, QobuzError> {
        Ok(self.album.clone())
    }

    async fn album_ref(&self, _album_id: &str) -> Result<AlbumDetail, QobuzError> {
        Ok(self.album.clone())
    }

    async fn album_search(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<AlbumSummary>, QobuzError> {
        Ok(vec![])
    }

    async fn artist_albums(&self, _artist_id: u64) -> Result<Vec<AlbumSummary>, QobuzError> {
        unimplemented!()
    }
}

pub async fn state_with_download_mock(mock: DownloadMockQobuz) -> AppState {
    use euterpe_server::config::AppConfig;
    use euterpe_server::crypto::MasterKey;
    use euterpe_server::db;
    use euterpe_server::services::download::{WorkerDeps, spawn_worker};
    use reqwest::Client;

    let library_path = std::env::temp_dir().join(format!("euterpe-dl-test-{}", std::process::id()));
    let config = AppConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        database_url: "sqlite::memory:".into(),
        admin_password: None,
        master_key: Some(MasterKey::parse(&hex::encode([1u8; 32])).unwrap()),
        public_base_url: "http://127.0.0.1:8080".into(),
        oauth_state_ttl: std::time::Duration::from_secs(600),
        qobuz_api_base: None,
        qobuz_play_base: None,
        library_path,
        torrent_incoming_dir: None,
        torrent_max_active: 2,
        torrent_enable_upnp: false,
        download_concurrency: 2,
        library_scan: euterpe_server::config::LibraryScanConfig::default(),
        debug: false,
        static_dir: std::path::PathBuf::new(),
    };
    let pool = db::connect(&config.database_url).await.unwrap();
    db::migrate(&pool).await.unwrap();
    euterpe_server::services::app_settings::save_storage(
        &pool,
        &euterpe_server::services::app_settings::StorageSettings::local(
            config.library_path.display().to_string(),
        ),
    )
    .await
    .unwrap();

    let (job_tx, job_rx) = mpsc::channel(32);
    let (convert_job_tx, _) = mpsc::channel(32);
    let (events, _) = broadcast::channel(16);
    let (scan_events, _) = broadcast::channel(16);
    let (convert_events, _) = broadcast::channel(16);
    let qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> = Arc::new(Mutex::new(Box::new(mock)));
    let config = Arc::new(config);
    let runtime = Arc::new(tokio::sync::RwLock::new(
        euterpe_server::services::app_settings::load_runtime_settings(&pool, &config).await,
    ));

    let state = AppState {
        db: pool.clone(),
        config: Arc::clone(&config),
        http: Client::new(),
        qobuz: Arc::clone(&qobuz),
        job_tx,
        convert_job_tx: convert_job_tx.clone(),
        events: events.clone(),
        scan_events: scan_events.clone(),
        convert_events,
        runtime: runtime.clone(),
        storage_watch: euterpe_server::services::storage_watch::StorageWatchHandle::new(
            euterpe_server::services::storage_watch::StorageWatchDeps {
                pool: pool.clone(),
                config: Arc::clone(&config),
                runtime,
                scan_events,
                convert_job_tx,
            },
        ),
        hawk: None,
        torrent: None,
        torrent_staging: Arc::new(euterpe_server::services::torrent_staging::TorrentStaging::new()),
    };

    qobuz_account::seed_active_qobuz_account(&state, 1, "test-token").await;

    let job_tx_wake = state.job_tx.clone();
    spawn_worker(
        job_rx,
        WorkerDeps {
            pool,
            qobuz,
            config,
            runtime: state.runtime.clone(),
            events,
            http: Client::new(),
            torrent: None,
            torrent_semaphore: None,
            scan_events: state.scan_events.clone(),
            job_tx: job_tx_wake.clone(),
        },
    );
    let _ = job_tx_wake.send(0).await;

    state
}

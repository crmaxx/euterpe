use std::sync::Arc;

use async_trait::async_trait;
use euterpe_qobuz::{
    AlbumDetail, AlbumSummary, ArtistRef, Page, PageRequest, QobuzApi, QobuzError, Quality,
    StreamUrl, TrackSummary,
};
pub use euterpe_server::app::test_support::test_state;
use euterpe_server::AppState;
use tokio::sync::Mutex;

pub struct MockQobuz {
    pub albums: Arc<Mutex<Vec<AlbumSummary>>>,
    pub fail_sync: bool,
    /// Returned from `album_ref` when set.
    pub album_ref_detail: Option<AlbumDetail>,
}

impl MockQobuz {
    pub fn with_albums(albums: Vec<AlbumSummary>) -> Self {
        Self {
            albums: Arc::new(Mutex::new(albums)),
            fail_sync: false,
            album_ref_detail: None,
        }
    }

    pub fn with_album_ref_detail(detail: AlbumDetail) -> Self {
        Self {
            albums: Arc::new(Mutex::new(vec![])),
            fail_sync: false,
            album_ref_detail: Some(detail),
        }
    }

    pub fn album(id: u64, title: &str, artist: &str) -> AlbumSummary {
        use euterpe_qobuz::ArtistRef;
        AlbumSummary {
            id,
            qobuz_id: None,
            title: title.into(),
            artist: Some(ArtistRef {
                id: 1,
                name: artist.into(),
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
        }
    }
}

#[async_trait]
impl QobuzApi for MockQobuz {
    async fn favorites_albums(
        &self,
        _page: PageRequest,
    ) -> Result<Page<AlbumSummary>, QobuzError> {
        unimplemented!()
    }

    async fn favorites_all_albums(&self) -> Result<Vec<AlbumSummary>, QobuzError> {
        if self.fail_sync {
            return Err(QobuzError::Authentication("mock auth fail".into()));
        }
        Ok(self.albums.lock().await.clone())
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
        unimplemented!()
    }

    async fn album(&self, _album_id: u64) -> Result<AlbumDetail, QobuzError> {
        unimplemented!()
    }

    async fn album_ref(&self, _album_id: &str) -> Result<AlbumDetail, QobuzError> {
        self.album_ref_detail
            .clone()
            .ok_or_else(|| QobuzError::NotFound {
                endpoint: "album/get".into(),
                message: "album_ref not configured in mock".into(),
            })
    }

    async fn album_search(&self, _query: &str, _limit: u32) -> Result<Vec<AlbumSummary>, QobuzError> {
        Ok(vec![])
    }

    async fn artist_albums(&self, _artist_id: u64) -> Result<Vec<AlbumSummary>, QobuzError> {
        unimplemented!()
    }
}

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
    async fn favorites_albums(
        &self,
        _page: PageRequest,
    ) -> Result<Page<AlbumSummary>, QobuzError> {
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

    async fn album_search(&self, _query: &str, _limit: u32) -> Result<Vec<AlbumSummary>, QobuzError> {
        Ok(vec![])
    }

    async fn artist_albums(&self, _artist_id: u64) -> Result<Vec<AlbumSummary>, QobuzError> {
        unimplemented!()
    }
}

pub async fn seed_active_qobuz_account(state: &AppState, user_id: u64, token: &str) {
    use euterpe_qobuz::OAuthLoginResult;
    use euterpe_server::credentials;

    let master = state.master_key().expect("master key in test state");
    let login = OAuthLoginResult {
        user_id,
        user_auth_token: token.to_string(),
        display_name: None,
        membership_label: None,
    };
    credentials::persist_oauth_account(&state.db, master, &login)
        .await
        .expect("seed account");
}

pub async fn state_with_download_mock(mock: DownloadMockQobuz) -> AppState {
    use euterpe_server::config::AppConfig;
    use euterpe_server::crypto::MasterKey;
    use euterpe_server::db;
    use euterpe_server::services::download::{spawn_worker, WorkerDeps};
    use reqwest::Client;
    use tokio::sync::{broadcast, mpsc};

    let library_path =
        std::env::temp_dir().join(format!("euterpe-dl-test-{}", std::process::id()));
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
        download_concurrency: 2,
        library_scan: euterpe_server::config::LibraryScanConfig::default(),
        debug: false,
        static_dir: std::path::PathBuf::new(),
    };
    let pool = db::connect(&config.database_url).await.unwrap();
    db::migrate(&pool).await.unwrap();

    let (job_tx, job_rx) = mpsc::channel(32);
    let (events, _) = broadcast::channel(16);
    let (scan_events, _) = broadcast::channel(16);
    let qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> = Arc::new(Mutex::new(Box::new(mock)));
    let config = Arc::new(config);

    let state = AppState {
        db: pool.clone(),
        config: Arc::clone(&config),
        http: Client::new(),
        qobuz: Arc::clone(&qobuz),
        job_tx,
        events: events.clone(),
        scan_events,
        hawk: None,
    };

    seed_active_qobuz_account(&state, 1, "test-token").await;

    spawn_worker(
        job_rx,
        WorkerDeps {
            pool,
            qobuz,
            config,
            events,
            http: Client::new(),
        },
    );

    state
}

pub async fn state_with_mock(mock: MockQobuz) -> AppState {
    let mut state = test_state().await;
    seed_active_qobuz_account(&state, 1, "test-token").await;
    state.qobuz = Arc::new(Mutex::new(
        Box::new(mock) as Box<dyn QobuzApi + Send + Sync>,
    ));
    state
}

pub fn load_spec() -> serde_json::Value {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../../openapi/openapi.yaml")).unwrap();
    serde_json::to_value(yaml).unwrap()
}

pub fn schema_from_spec(spec: &serde_json::Value, name: &str) -> serde_json::Value {
    let schema = spec
        .pointer(&format!("/components/schemas/{name}"))
        .cloned()
        .unwrap_or_else(|| panic!("schema {name} not found"));
    resolve_refs(spec, &schema)
}

/// Resolve `#/components/schemas/*` refs for jsonschema validation.
fn resolve_refs(spec: &serde_json::Value, schema: &serde_json::Value) -> serde_json::Value {
    if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
        if let Some(name) = ref_path.strip_prefix("#/components/schemas/") {
            let target = spec
                .pointer(&format!("/components/schemas/{name}"))
                .expect("ref target");
            return resolve_refs(spec, target);
        }
    }

    match schema {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if k == "$ref" {
                    continue;
                }
                out.insert(k.clone(), resolve_refs(spec, v));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| resolve_refs(spec, v)).collect())
        }
        _ => schema.clone(),
    }
}

pub fn validate_schema(schema: &serde_json::Value, instance: &serde_json::Value) {
    let validator = jsonschema::validator_for(schema).expect("valid jsonschema");
    if let Err(error) = validator.validate(instance) {
        panic!("schema validation failed: {error}");
    }
}

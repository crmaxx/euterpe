use std::sync::Arc;

use async_trait::async_trait;
use euterpe_qobuz::{
    AlbumDetail, AlbumSummary, Page, PageRequest, QobuzApi, QobuzError, Quality, StreamUrl,
};
use euterpe_server::AppState;
use euterpe_server::app::test_support::test_state;
use tokio::sync::Mutex;

#[path = "qobuz_account.rs"]
mod qobuz_account;

pub struct MockQobuz {
    pub albums: Arc<Mutex<Vec<AlbumSummary>>>,
    pub fail_sync: bool,
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
    async fn favorites_albums(&self, _page: PageRequest) -> Result<Page<AlbumSummary>, QobuzError> {
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

pub async fn state_with_mock(mock: MockQobuz) -> AppState {
    let mut state = test_state().await;
    qobuz_account::seed_active_qobuz_account(&state, 1, "test-token").await;
    state.qobuz = Arc::new(Mutex::new(Box::new(mock) as Box<dyn QobuzApi + Send + Sync>));
    state
}

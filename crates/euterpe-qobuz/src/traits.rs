use async_trait::async_trait;

use crate::error::QobuzError;
use crate::models::{AlbumDetail, AlbumSummary, StreamUrl};
use crate::pagination::PageRequest;
use crate::api::streaming::Quality;
use crate::client::QobuzClient;
use crate::pagination::Page;

#[async_trait]
pub trait QobuzApi: Send + Sync {
    async fn favorites_albums(
        &self,
        page: PageRequest,
    ) -> Result<Page<AlbumSummary>, QobuzError>;

    async fn favorites_all_albums(&self) -> Result<Vec<AlbumSummary>, QobuzError>;

    async fn favorites_album_api_id_for_catalog(
        &self,
        catalog_id: u64,
    ) -> Result<Option<String>, QobuzError>;

    async fn favorite_add_albums(&self, ids: &[u64]) -> Result<(), QobuzError>;

    async fn favorite_remove_albums(&self, ids: &[u64]) -> Result<(), QobuzError>;

    async fn track_stream_url(
        &mut self,
        track_id: u64,
        quality: Quality,
    ) -> Result<StreamUrl, QobuzError>;

    async fn album(&self, album_id: u64) -> Result<AlbumDetail, QobuzError>;

    async fn album_ref(&self, album_id: &str) -> Result<AlbumDetail, QobuzError>;

    async fn album_search(&self, query: &str, limit: u32) -> Result<Vec<AlbumSummary>, QobuzError>;

    async fn artist_albums(&self, artist_id: u64) -> Result<Vec<AlbumSummary>, QobuzError>;
}

#[async_trait]
impl QobuzApi for QobuzClient {
    async fn favorites_albums(
        &self,
        page: PageRequest,
    ) -> Result<Page<AlbumSummary>, QobuzError> {
        self.favorites_albums(page).await
    }

    async fn favorites_all_albums(&self) -> Result<Vec<AlbumSummary>, QobuzError> {
        self.favorites_all_albums().await
    }

    async fn favorites_album_api_id_for_catalog(
        &self,
        catalog_id: u64,
    ) -> Result<Option<String>, QobuzError> {
        self.favorites_album_api_id_for_catalog(catalog_id).await
    }

    async fn favorite_add_albums(&self, ids: &[u64]) -> Result<(), QobuzError> {
        self.favorite_add_albums(ids).await
    }

    async fn favorite_remove_albums(&self, ids: &[u64]) -> Result<(), QobuzError> {
        self.favorite_remove_albums(ids).await
    }

    async fn track_stream_url(
        &mut self,
        track_id: u64,
        quality: Quality,
    ) -> Result<StreamUrl, QobuzError> {
        self.track_stream_url(track_id, quality).await
    }

    async fn album(&self, album_id: u64) -> Result<AlbumDetail, QobuzError> {
        self.album(album_id).await
    }

    async fn album_ref(&self, album_id: &str) -> Result<AlbumDetail, QobuzError> {
        self.album_ref(album_id).await
    }

    async fn album_search(&self, query: &str, limit: u32) -> Result<Vec<AlbumSummary>, QobuzError> {
        self.album_search(query, limit).await
    }

    async fn artist_albums(&self, artist_id: u64) -> Result<Vec<AlbumSummary>, QobuzError> {
        self.artist_albums(artist_id).await
    }
}

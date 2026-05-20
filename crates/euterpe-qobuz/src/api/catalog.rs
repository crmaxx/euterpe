use crate::client::QobuzClient;
use crate::error::QobuzError;
use crate::models::{AlbumDetail, AlbumSummary};
use crate::pagination::{PageRequest, fetch_all_pages};

#[derive(Debug, serde::Deserialize)]
struct AlbumSearchResponse {
    albums: AlbumSearchBlock,
}

#[derive(Debug, serde::Deserialize)]
struct AlbumSearchBlock {
    items: Vec<AlbumSummary>,
}

#[derive(Debug, serde::Deserialize)]
struct ArtistAlbumsPage {
    albums: ArtistAlbumsBlock,
}

#[derive(Debug, serde::Deserialize)]
struct ArtistAlbumsBlock {
    items: Vec<AlbumSummary>,
    total: u32,
    limit: u32,
    offset: u32,
}

impl QobuzClient {
    pub async fn album(&self, album_id: u64) -> Result<AlbumDetail, QobuzError> {
        self.album_ref(&album_id.to_string()).await
    }

    /// `album_id` is catalog numeric id or Qobuz URL slug (see `AlbumSummary::api_album_id`).
    pub async fn album_ref(&self, album_id: &str) -> Result<AlbumDetail, QobuzError> {
        let mut params = vec![
            ("app_id", self.state.app_id.clone()),
            ("album_id", album_id.to_string()),
            ("limit", "500".to_string()),
            ("offset", "0".to_string()),
            ("extra", "track_ids,albumsFromSameArtist".to_string()),
        ];
        if let Some(uat) = &self.state.user_auth_token {
            params.push(("user_auth_token", uat.clone()));
        }
        if let crate::config::AuthConfig::SessionToken { user_id, .. }
        | crate::config::AuthConfig::TokenLogin { user_id, .. } = &self.config.auth
        {
            params.push(("user_id", user_id.to_string()));
        }

        let (status, body) = self.get_json("album/get", &params).await?;
        if status != 200 {
            return Err(QobuzError::from_status("album/get", status, &body));
        }
        serde_json::from_value(body).map_err(QobuzError::from)
    }

    pub async fn album_search(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<AlbumSummary>, QobuzError> {
        let mut params = vec![
            ("app_id", self.state.app_id.clone()),
            ("query", query.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(uat) = &self.state.user_auth_token {
            params.push(("user_auth_token", uat.clone()));
        }
        if let crate::config::AuthConfig::SessionToken { user_id, .. }
        | crate::config::AuthConfig::TokenLogin { user_id, .. } = &self.config.auth
        {
            params.push(("user_id", user_id.to_string()));
        }

        let (status, body) = self.get_json("album/search", &params).await?;
        if status != 200 {
            return Err(QobuzError::from_status("album/search", status, &body));
        }
        let parsed: AlbumSearchResponse = serde_json::from_value(body)?;
        Ok(parsed.albums.items)
    }

    pub async fn artist_albums(&self, artist_id: u64) -> Result<Vec<AlbumSummary>, QobuzError> {
        fetch_all_pages(|page| self.artist_albums_page(artist_id, page)).await
    }

    async fn artist_albums_page(
        &self,
        artist_id: u64,
        page: PageRequest,
    ) -> Result<crate::pagination::Page<AlbumSummary>, QobuzError> {
        let params = vec![
            ("app_id", self.state.app_id.clone()),
            ("artist_id", artist_id.to_string()),
            ("extra", "albums".to_string()),
            ("limit", page.limit.to_string()),
            ("offset", page.offset.to_string()),
        ];
        let (status, body) = self.get_json("artist/get", &params).await?;
        if status != 200 {
            return Err(QobuzError::from_status("artist/get", status, &body));
        }
        let parsed: ArtistAlbumsPage = serde_json::from_value(body)?;
        Ok(crate::pagination::Page {
            items: parsed.albums.items,
            total: parsed.albums.total,
            limit: parsed.albums.limit,
            offset: parsed.albums.offset,
        })
    }
}

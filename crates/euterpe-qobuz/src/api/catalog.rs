use crate::client::QobuzClient;
use crate::error::QobuzError;
use crate::models::{AlbumDetail, AlbumSummary};
use crate::pagination::{fetch_all_pages, PageRequest};

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
        let params = vec![("album_id", album_id.to_string())];
        let (status, body) = self.get_json("album/get", &params).await?;
        if status != 200 {
            return Err(QobuzError::from_status("album/get", status, &body));
        }
        serde_json::from_value(body).map_err(QobuzError::from)
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

use std::sync::Arc;

use euterpe_qobuz::{QobuzApi, QobuzError};
use tokio::sync::Mutex;

use crate::db::favorites;
use crate::error::ApiError;
use crate::state::AppState;
use sqlx::SqlitePool;

/// Best `album_id` for Qobuz `album/get` (short ref, long slug, or catalog id string).
pub async fn resolve_album_api_id(
    pool: &SqlitePool,
    qobuz: &Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>>,
    catalog_id: u64,
    from_request: Option<&str>,
) -> Result<Option<String>, ApiError> {
    if let Some(id) = from_request.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(Some(id.to_string()));
    }

    if let Some(meta) = favorites::album_meta(pool, catalog_id).await? {
        if let Some(id) = meta.slug.filter(|s| !s.trim().is_empty()) {
            if id.parse::<u64>().ok() != Some(catalog_id) {
                return Ok(Some(id));
            }
        }
    }

    let guard = qobuz.lock().await;
    resolve_from_qobuz_favorites(&**guard, catalog_id)
        .await
        .map_err(Into::into)
}

pub async fn resolve_album_api_id_for_state(
    state: &AppState,
    catalog_id: u64,
    from_request: Option<&str>,
) -> Result<Option<String>, ApiError> {
    resolve_album_api_id(&state.db, &state.qobuz, catalog_id, from_request).await
}

/// Resolve via Qobuz favorites (JSON scan + parsed list). No `album/get` probe loop.
pub async fn resolve_from_qobuz_favorites(
    qobuz: &dyn QobuzApi,
    catalog_id: u64,
) -> Result<Option<String>, QobuzError> {
    qobuz.favorites_album_api_id_for_catalog(catalog_id).await
}

pub fn push_album_api_candidate(candidates: &mut Vec<String>, id: &str) {
    let t = id.trim();
    if t.is_empty() || candidates.iter().any(|c| c == t) {
        return;
    }
    candidates.push(t.to_string());
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use euterpe_qobuz::{
        AlbumDetail, AlbumSummary, Page, PageRequest, QobuzApi, QobuzError, Quality, StreamUrl,
    };
    use tokio::sync::Mutex;

    use super::*;
    use crate::db;

    struct ResolveMockQobuz {
        catalog_lookup: Option<(u64, String)>,
    }

    #[async_trait]
    impl QobuzApi for ResolveMockQobuz {
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
            catalog_id: u64,
        ) -> Result<Option<String>, QobuzError> {
            Ok(self
                .catalog_lookup
                .as_ref()
                .filter(|(id, _)| *id == catalog_id)
                .map(|(_, api)| api.clone()))
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
            unimplemented!()
        }

        async fn album_search(
            &self,
            _query: &str,
            _limit: u32,
        ) -> Result<Vec<AlbumSummary>, QobuzError> {
            Ok(vec![])
        }

        async fn artist_albums(&self, _id: u64) -> Result<Vec<AlbumSummary>, QobuzError> {
            unimplemented!()
        }
    }

    async fn pool() -> SqlitePool {
        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn resolve_prefers_explicit_request_id() {
        let pool = pool().await;
        let qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> =
            Arc::new(Mutex::new(Box::new(ResolveMockQobuz {
            catalog_lookup: Some((99, "from-qobuz".into())),
        })));
        let got = resolve_album_api_id(&pool, &qobuz, 99, Some("from-request"))
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some("from-request"));
    }

    #[tokio::test]
    async fn resolve_uses_local_favorite_slug_before_qobuz() {
        let pool = pool().await;
        favorites::upsert_album(&pool, 42, "Album", "Artist", Some("local-ref"), None)
            .await
            .unwrap();
        let qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> =
            Arc::new(Mutex::new(Box::new(ResolveMockQobuz {
            catalog_lookup: Some((42, "qobuz-ref".into())),
        })));
        let got = resolve_album_api_id(&pool, &qobuz, 42, None)
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some("local-ref"));
    }

    #[tokio::test]
    async fn resolve_falls_back_to_favorites_catalog_scan() {
        let pool = pool().await;
        let qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> =
            Arc::new(Mutex::new(Box::new(ResolveMockQobuz {
            catalog_lookup: Some((393908828, "zg7pv28g4mldg".into())),
        })));
        let got = resolve_album_api_id(&pool, &qobuz, 393908828, None)
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some("zg7pv28g4mldg"));
    }
}

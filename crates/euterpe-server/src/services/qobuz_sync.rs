use std::collections::HashSet;
use std::sync::Arc;

use euterpe_qobuz::{AlbumSummary, QobuzApi};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::api::QobuzSyncResponse;
use crate::db::{favorites, sync_runs};
use crate::error::ApiError;

fn album_fields(album: &AlbumSummary) -> (u64, String, String, Option<String>, Option<String>) {
    let artist = album
        .artist
        .as_ref()
        .map(|a| a.name.clone())
        .or_else(|| {
            album
                .artists
                .as_ref()
                .and_then(|v| v.first())
                .map(|a| a.name.clone())
        })
        .unwrap_or_default();
    let album_api_id = album
        .pick_album_api_id(album.id)
        .unwrap_or_else(|| album.api_album_id());
    let cover_url = album.image.as_ref().and_then(|img| {
        img.thumbnail
            .clone()
            .or_else(|| img.small.clone())
            .filter(|s| !s.trim().is_empty())
    });
    (
        album.id,
        album.title.clone(),
        artist,
        Some(album_api_id),
        cover_url,
    )
}

pub async fn run(
    pool: &SqlitePool,
    qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>>,
) -> Result<QobuzSyncResponse, ApiError> {
    let run_id = sync_runs::start(pool).await?;

    let sync_result: Result<QobuzSyncResponse, ApiError> = async {
        let albums = {
            let guard = qobuz.lock().await;
            guard.favorites_all_albums().await?
        };

        let before = favorites::active_album_ids(pool).await?;
        let before_set: HashSet<u64> = before.iter().copied().collect();

        let mut added = 0u64;
        for album in &albums {
            let (qobuz_id, title, artist, album_api_id, cover_url) = album_fields(album);
            let existed = before_set.contains(&qobuz_id);
            favorites::upsert_album(
                pool,
                qobuz_id,
                &title,
                &artist,
                album_api_id.as_deref(),
                cover_url.as_deref(),
            )
            .await?;
            if !existed {
                added += 1;
            }
        }

        let keep_ids: Vec<u64> = albums.iter().map(|a| album_fields(a).0).collect();
        let removed = favorites::mark_removed_except(pool, &keep_ids).await?;

        let albums_total = albums.len() as i64;
        sync_runs::finish_success(pool, run_id, albums_total, added as i64, removed as i64)
            .await?;

        Ok(QobuzSyncResponse {
            run_id,
            albums_total,
            added: added as i64,
            removed: removed as i64,
        })
    }
    .await;

    match sync_result {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let _ = sync_runs::finish_failed(pool, run_id, &e.to_string()).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use euterpe_qobuz::{AlbumSummary, Page, PageRequest, QobuzApi, QobuzError, StreamUrl};
    use euterpe_qobuz::Quality;

    use super::*;
    use crate::db;

    struct MockQobuz {
        albums: Arc<tokio::sync::Mutex<Vec<AlbumSummary>>>,
    }

    impl MockQobuz {
        fn new(albums: Vec<AlbumSummary>) -> Self {
            Self {
                albums: Arc::new(tokio::sync::Mutex::new(albums)),
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

        async fn album(&self, _album_id: u64) -> Result<euterpe_qobuz::AlbumDetail, QobuzError> {
            unimplemented!()
        }

        async fn album_ref(&self, _album_id: &str) -> Result<euterpe_qobuz::AlbumDetail, QobuzError> {
            unimplemented!()
        }

        async fn album_search(
            &self,
            _query: &str,
            _limit: u32,
        ) -> Result<Vec<AlbumSummary>, QobuzError> {
            Ok(vec![])
        }

        async fn artist_albums(
            &self,
            _artist_id: u64,
        ) -> Result<Vec<AlbumSummary>, QobuzError> {
            unimplemented!()
        }
    }

    fn album(id: u64, title: &str) -> AlbumSummary {
        AlbumSummary {
            id,
            qobuz_id: None,
            title: title.into(),
            artist: Some(euterpe_qobuz::ArtistRef {
                id: 1,
                name: "A".into(),
            }),
            artists: None,
            image: None,
            release_date_original: None,
            hires: None,
            album_ref: None,
            slug: None,
            list_id: None,
            genre: None,
            label: None,
        }
    }

    async fn test_pool() -> SqlitePool {
        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn sync_persists_album_api_id_for_album_get() {
        let pool = test_pool().await;
        let album = AlbumSummary {
            id: 393908828,
            qobuz_id: Some(393908828),
            title: "Lutosławski".into(),
            artist: Some(euterpe_qobuz::ArtistRef {
                id: 1,
                name: "Luxembourg Philharmonic".into(),
            }),
            artists: None,
            image: None,
            release_date_original: None,
            hires: Some(true),
            album_ref: Some("zg7pv28g4mldg".into()),
            slug: Some("lutosawski-concertos-for-cello".into()),
            list_id: Some(3149020953969),
            genre: None,
            label: None,
        };
        let mock: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> =
            Arc::new(Mutex::new(Box::new(MockQobuz::new(vec![album]))
                as Box<dyn QobuzApi + Send + Sync>));

        run(&pool, mock).await.unwrap();

        let meta = favorites::album_meta(&pool, 393908828)
            .await
            .unwrap()
            .expect("album in db");
        assert_eq!(meta.slug.as_deref(), Some("zg7pv28g4mldg"));
    }

    #[tokio::test]
    async fn sync_diff_marks_removed() {
        let pool = test_pool().await;
        let inner = MockQobuz::new(vec![album(1, "A"), album(2, "B")]);
        let albums = Arc::clone(&inner.albums);
        let mock: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>> = Arc::new(Mutex::new(
            Box::new(inner) as Box<dyn QobuzApi + Send + Sync>,
        ));

        let r1 = run(&pool, Arc::clone(&mock)).await.unwrap();
        assert_eq!(r1.albums_total, 2);
        assert_eq!(r1.added, 2);

        *albums.lock().await = vec![album(1, "A")];
        let r2 = run(&pool, mock).await.unwrap();
        assert_eq!(r2.removed, 1);
        assert_eq!(r2.added, 0);

        use crate::api::SortOrder;
        use crate::db::favorites::{FavoritesListParams, FavoritesSort};
        let page = favorites::list_albums_keyset(
            &pool,
            FavoritesListParams {
                sort: FavoritesSort::Title,
                order: SortOrder::Asc,
                limit: 50,
                q: None,
                in_library: None,
                cursor: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].qobuz_id, 1);
    }
}

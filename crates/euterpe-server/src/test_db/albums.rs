use serde_json::json;
use sqlx::SqlitePool;

use crate::api::keyset::{
    decode_cursor, ensure_cursor_matches, fingerprint_json, finish_keyset_page,
};
use crate::api::{KeysetPage, SortKeyKind, SortKeyValue, SortOrder};
use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::catalog;

#[derive(Debug, Clone)]
pub struct AlbumRow {
    pub id: i64,
    pub artist_id: Option<i64>,
    pub title: String,
    pub year: Option<i32>,
    pub qobuz_album_id: Option<i64>,
    pub path: Option<String>,
    pub cover_path: Option<String>,
}

pub struct AlbumUpsert<'a> {
    pub artist_id: Option<i64>,
    pub title: &'a str,
    pub year: Option<i32>,
    pub qobuz_album_id: Option<i64>,
    pub path: Option<&'a str>,
    pub cover_path: Option<&'a str>,
}

pub async fn upsert(pool: &SqlitePool, album: AlbumUpsert<'_>) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::upsert_album(
        &handle,
        catalog::AlbumUpsert {
            artist_id: album.artist_id,
            title: album.title,
            year: album.year,
            qobuz_album_id: album.qobuz_album_id,
            path: album.path,
            cover_path: album.cover_path,
        },
    )
    .await?)
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<AlbumRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::get_album_by_id(&handle, id)
        .await?
        .map(album_row_from_data))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumsSort {
    Title,
    Artist,
    AlbumDate,
    DateAdded,
}

impl AlbumsSort {
    pub fn parse(s: &str) -> Result<Self, ApiError> {
        match s {
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            "album_date" => Ok(Self::AlbumDate),
            "date_added" => Ok(Self::DateAdded),
            _ => Err(ApiError::bad_request(
                "sort must be title, artist, album_date, or date_added",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::AlbumDate => "album_date",
            Self::DateAdded => "date_added",
        }
    }

    fn key_kind(self) -> SortKeyKind {
        match self {
            Self::AlbumDate => SortKeyKind::Int,
            _ => SortKeyKind::Text,
        }
    }

    fn primary_key(self, row: &AlbumListRow, order: SortOrder) -> SortKeyValue {
        match self {
            Self::Title => SortKeyValue::Text(row.title.clone()),
            Self::Artist => SortKeyValue::Text(row.artist_name.clone()),
            Self::AlbumDate => SortKeyValue::Int(album_date_sort_value(row.year, order)),
            Self::DateAdded => SortKeyValue::Text(row.created_at.clone()),
        }
    }
}

fn album_date_sort_value(year: Option<i32>, order: SortOrder) -> i64 {
    catalog::album_date_sort_value(year, sort_order_to_data(order))
}

#[derive(Debug, Clone)]
pub struct AlbumsListParams {
    pub sort: AlbumsSort,
    pub order: SortOrder,
    pub limit: u32,
    pub q: Option<String>,
    pub cursor: Option<String>,
}

pub async fn list_keyset(
    pool: &SqlitePool,
    mut params: AlbumsListParams,
) -> Result<KeysetPage<AlbumListRow>, ApiError> {
    params.q = catalog::normalize_album_search_query(params.q);
    let fingerprint = fingerprint_json(&json!({ "q": params.q }));

    let after = if let Some(ref cursor_str) = params.cursor {
        let payload = decode_cursor(cursor_str)?;
        let (primary, tie) = ensure_cursor_matches(
            &payload,
            params.sort.as_str(),
            params.order,
            &fingerprint,
            params.sort.key_kind(),
        )?;
        Some(catalog::AlbumListCursor {
            primary: album_sort_value_to_data(primary),
            tie_id: tie,
        })
    } else {
        None
    };

    let handle = DataHandle::from_sqlite_pool(pool.clone());
    let page = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: album_sort_to_data(params.sort),
            order: sort_order_to_data(params.order),
            limit: params.limit as usize + 1,
            q: params.q.clone(),
            after,
        },
    )
    .await?;
    let sort = params.sort;
    Ok(finish_keyset_page(
        page.items
            .into_iter()
            .map(album_list_row_from_data)
            .collect(),
        params.limit as usize,
        sort.as_str(),
        params.order,
        &fingerprint,
        |r| (sort.primary_key(r, params.order), r.id),
    ))
}

#[derive(Debug, Clone)]
pub struct AlbumListRow {
    pub id: i64,
    pub title: String,
    pub artist_name: String,
    pub year: Option<i32>,
    pub created_at: String,
    pub path: Option<String>,
    pub cover_path: Option<String>,
    pub track_count: i64,
}

pub async fn set_cover_path(pool: &SqlitePool, id: i64, cover_path: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    if !catalog::set_album_cover_path(&handle, id, cover_path).await? {
        return Err(ApiError::Message("album not found".into()));
    }
    Ok(())
}

pub async fn id_by_path(pool: &SqlitePool, path: &str) -> Result<Option<i64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::album_id_by_path(&handle, path).await?)
}

pub async fn delete_empty_storage_albums_in_scope(
    pool: &SqlitePool,
    scope_path: Option<&str>,
) -> Result<u64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::delete_empty_storage_albums_in_scope(&handle, scope_path).await?)
}

pub(crate) fn path_prefix_bounds(path: &str) -> (String, String) {
    let lower = format!("{}/", path.trim_end_matches('/'));
    let mut upper = lower.clone().into_bytes();
    if let Some(last) = upper.last_mut() {
        *last = last.saturating_add(1);
    }
    (
        lower,
        String::from_utf8(upper).expect("path prefix upper bound remains valid UTF-8"),
    )
}

pub async fn find_id_by_qobuz_album_id(
    pool: &SqlitePool,
    qobuz_id: i64,
) -> Result<Option<i64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::find_album_id_by_qobuz_album_id(&handle, qobuz_id).await?)
}

fn album_row_from_data(row: catalog::AlbumRow) -> AlbumRow {
    AlbumRow {
        id: row.id,
        artist_id: row.artist_id,
        title: row.title,
        year: row.year,
        qobuz_album_id: row.qobuz_album_id,
        path: row.path,
        cover_path: row.cover_path,
    }
}

fn album_list_row_from_data(row: catalog::AlbumListRow) -> AlbumListRow {
    AlbumListRow {
        id: row.id,
        title: row.title,
        artist_name: row.artist_name,
        year: row.year,
        created_at: row.created_at,
        path: row.path,
        cover_path: row.cover_path,
        track_count: row.track_count,
    }
}

fn album_sort_to_data(sort: AlbumsSort) -> catalog::AlbumListSort {
    match sort {
        AlbumsSort::Title => catalog::AlbumListSort::Title,
        AlbumsSort::Artist => catalog::AlbumListSort::Artist,
        AlbumsSort::AlbumDate => catalog::AlbumListSort::AlbumDate,
        AlbumsSort::DateAdded => catalog::AlbumListSort::DateAdded,
    }
}

fn sort_order_to_data(order: SortOrder) -> catalog::AlbumListOrder {
    match order {
        SortOrder::Asc => catalog::AlbumListOrder::Asc,
        SortOrder::Desc => catalog::AlbumListOrder::Desc,
    }
}

fn album_sort_value_to_data(value: SortKeyValue) -> catalog::AlbumListSortValue {
    match value {
        SortKeyValue::Text(value) => catalog::AlbumListSortValue::Text(value),
        SortKeyValue::Int(value) => catalog::AlbumListSortValue::Int(value),
        SortKeyValue::Bool(value) => catalog::AlbumListSortValue::Int(value as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::{artists, connect, migrate};

    #[tokio::test]
    async fn upsert_album_by_path() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let artist_id = artists::upsert_by_name(&pool, "A", None).await.unwrap();
        let id1 = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album",
                year: Some(2020),
                qobuz_album_id: None,
                path: Some("/music/A/Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let id2 = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album Updated",
                year: Some(2021),
                qobuz_album_id: None,
                path: Some("/music/A/Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(id1, id2);
        let row = get_by_id(&pool, id1).await.unwrap().unwrap();
        assert_eq!(row.title, "Album Updated");
    }

    #[tokio::test]
    async fn delete_empty_storage_albums_in_scope_keeps_non_empty_and_metadata_only() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let artist_id = artists::upsert_by_name(&pool, "A", None).await.unwrap();
        let empty_in_scope = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Empty",
                year: None,
                qobuz_album_id: None,
                path: Some("A/Empty"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let non_empty = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Full",
                year: None,
                qobuz_album_id: None,
                path: Some("A/Full"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let empty_outside_scope = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Outside",
                year: None,
                qobuz_album_id: None,
                path: Some("B/Outside"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let metadata_only = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Metadata",
                year: None,
                qobuz_album_id: Some(42),
                path: None,
                cover_path: None,
            },
        )
        .await
        .unwrap();
        crate::test_db::tracks::upsert(
            &pool,
            crate::test_db::tracks::TrackUpsert {
                album_id: non_empty,
                title: "Track",
                track_number: None,
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: "A/Full/01.flac",
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();

        let deleted = delete_empty_storage_albums_in_scope(&pool, Some("A"))
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        assert!(get_by_id(&pool, empty_in_scope).await.unwrap().is_none());
        assert!(get_by_id(&pool, non_empty).await.unwrap().is_some());
        assert!(
            get_by_id(&pool, empty_outside_scope)
                .await
                .unwrap()
                .is_some()
        );
        assert!(get_by_id(&pool, metadata_only).await.unwrap().is_some());
    }
}

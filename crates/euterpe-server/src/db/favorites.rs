use euterpe_data::DataHandle;
use euterpe_data::repositories::favorites as data;
use serde_json::json;
use sqlx::SqlitePool;

use crate::api::keyset::{decode_cursor, encode_cursor, ensure_cursor_matches, fingerprint_json};
use crate::api::{KeysetPage, QobuzFavoriteItem, SortKeyKind, SortKeyValue, SortOrder};
use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoritesSort {
    Title,
    Artist,
    InLibrary,
}

impl FavoritesSort {
    pub fn parse(s: &str) -> Result<Self, ApiError> {
        match s {
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            "in_library" => Ok(Self::InLibrary),
            _ => Err(ApiError::bad_request(
                "sort must be title, artist, or in_library",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::InLibrary => "in_library",
        }
    }

    fn key_kind(self) -> SortKeyKind {
        match self {
            Self::InLibrary => SortKeyKind::Bool,
            _ => SortKeyKind::Text,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FavoritesListParams {
    pub sort: FavoritesSort,
    pub order: SortOrder,
    pub limit: u32,
    pub q: Option<String>,
    pub in_library: Option<bool>,
    pub cursor: Option<String>,
}

/// `album_api_id` is stored in `slug` column: short ref, long slug, or catalog id for `album/get`.
pub async fn upsert_album(
    pool: &SqlitePool,
    qobuz_id: u64,
    title: &str,
    artist_name: &str,
    album_api_id: Option<&str>,
    cover_url: Option<&str>,
) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::upsert_album(
        &handle,
        qobuz_id,
        title,
        artist_name,
        album_api_id,
        cover_url,
    )
    .await?)
}

#[derive(Debug, Clone)]
pub struct FavoriteAlbumMeta {
    pub slug: Option<String>,
    pub title: String,
    pub artist_name: String,
}

pub async fn album_meta(
    pool: &SqlitePool,
    qobuz_id: u64,
) -> Result<Option<FavoriteAlbumMeta>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::album_meta(&handle, qobuz_id)
        .await?
        .map(|meta| FavoriteAlbumMeta {
            slug: meta.slug,
            title: meta.title,
            artist_name: meta.artist_name,
        }))
}

pub async fn mark_removed_except(pool: &SqlitePool, keep_ids: &[u64]) -> Result<u64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::mark_removed_except(&handle, keep_ids).await?)
}

pub async fn list_albums_keyset(
    pool: &SqlitePool,
    params: FavoritesListParams,
) -> Result<KeysetPage<QobuzFavoriteItem>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    let fingerprint = fingerprint_json(&json!({
        "q": params.q,
        "in_library": params.in_library,
    }));
    let after = decode_favorites_cursor(&params, &fingerprint)?;
    let page = data::list_albums_keyset(
        &handle,
        data::FavoritesListParams {
            sort: data_sort(params.sort),
            order: data_order(params.order),
            limit: params.limit as usize,
            q: params.q,
            in_library: params.in_library,
            after,
        },
    )
    .await?;
    let next_cursor = page
        .next_after
        .as_ref()
        .map(|cursor| encode_favorites_cursor(params.sort, params.order, &fingerprint, cursor));
    Ok(KeysetPage {
        items: page
            .items
            .into_iter()
            .map(favorite_item_from_data)
            .collect(),
        next_cursor,
        has_more: page.has_more,
    })
}

pub async fn mark_albums_removed(pool: &SqlitePool, ids: &[u64]) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::mark_albums_removed(&handle, ids).await?)
}

pub async fn active_album_ids(pool: &SqlitePool) -> Result<Vec<u64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::active_album_ids(&handle).await?)
}

fn decode_favorites_cursor(
    params: &FavoritesListParams,
    fingerprint: &str,
) -> Result<Option<data::FavoriteListCursor>, ApiError> {
    let Some(cursor_str) = params.cursor.as_deref() else {
        return Ok(None);
    };
    let payload = decode_cursor(cursor_str)?;
    let (primary, tie_qobuz_id) = ensure_cursor_matches(
        &payload,
        params.sort.as_str(),
        params.order,
        fingerprint,
        params.sort.key_kind(),
    )?;
    Ok(Some(data::FavoriteListCursor {
        primary: data_sort_value(primary),
        tie_qobuz_id,
    }))
}

fn encode_favorites_cursor(
    sort: FavoritesSort,
    order: SortOrder,
    fingerprint: &str,
    cursor: &data::FavoriteListCursor,
) -> String {
    encode_cursor(
        sort.as_str(),
        order,
        fingerprint,
        &api_sort_value(&cursor.primary),
        cursor.tie_qobuz_id,
    )
}

fn data_sort(sort: FavoritesSort) -> data::FavoritesSort {
    match sort {
        FavoritesSort::Title => data::FavoritesSort::Title,
        FavoritesSort::Artist => data::FavoritesSort::Artist,
        FavoritesSort::InLibrary => data::FavoritesSort::InLibrary,
    }
}

fn data_order(order: SortOrder) -> data::SortOrder {
    match order {
        SortOrder::Asc => data::SortOrder::Asc,
        SortOrder::Desc => data::SortOrder::Desc,
    }
}

fn data_sort_value(value: SortKeyValue) -> data::FavoriteSortValue {
    match value {
        SortKeyValue::Text(text) => data::FavoriteSortValue::Text(text),
        SortKeyValue::Bool(value) => data::FavoriteSortValue::Bool(value),
        SortKeyValue::Int(value) => data::FavoriteSortValue::Text(value.to_string()),
    }
}

fn api_sort_value(value: &data::FavoriteSortValue) -> SortKeyValue {
    match value {
        data::FavoriteSortValue::Text(text) => SortKeyValue::Text(text.clone()),
        data::FavoriteSortValue::Bool(value) => SortKeyValue::Bool(*value),
    }
}

fn favorite_item_from_data(row: data::QobuzFavoriteAlbum) -> QobuzFavoriteItem {
    QobuzFavoriteItem {
        album_api_id: row.album_api_id,
        qobuz_id: row.qobuz_id,
        title: row.title,
        artist_name: row.artist_name,
        in_library: row.in_library,
        local_album_id: row.local_album_id,
        local_cover_path: row.local_cover_path,
        cover_url: row.cover_url,
    }
}

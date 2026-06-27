use std::cmp::Ordering;
use std::collections::HashMap;

use chrono::Utc;
use welds::WeldsModel;

use crate::connection::DataHandle;
use crate::error::{DataError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoritesSort {
    Title,
    Artist,
    InLibrary,
}

impl FavoritesSort {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            "in_library" => Ok(Self::InLibrary),
            _ => Err(DataError::InvalidOperation(
                "sort must be title, artist, or in_library".to_string(),
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FavoriteSortValue {
    Text(String),
    Bool(i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteListCursor {
    pub primary: FavoriteSortValue,
    pub tie_qobuz_id: i64,
}

#[derive(Debug, Clone)]
pub struct FavoritesListParams {
    pub sort: FavoritesSort,
    pub order: SortOrder,
    pub limit: usize,
    pub q: Option<String>,
    pub in_library: Option<bool>,
    pub after: Option<FavoriteListCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QobuzFavoriteAlbum {
    pub album_api_id: String,
    pub qobuz_id: i64,
    pub title: String,
    pub artist_name: String,
    pub in_library: bool,
    pub local_album_id: Option<i64>,
    pub local_cover_path: Option<String>,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QobuzFavoriteAlbumPage {
    pub items: Vec<QobuzFavoriteAlbum>,
    pub next_after: Option<FavoriteListCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteAlbumMeta {
    pub slug: Option<String>,
    pub title: String,
    pub artist_name: String,
}

#[derive(Debug, Clone)]
struct FavoriteJoinedRow {
    qobuz_id: i64,
    album_api_id: Option<String>,
    title: Option<String>,
    artist_name: Option<String>,
    cover_url: Option<String>,
    local_album_id: Option<i64>,
    local_cover_path: Option<String>,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "qobuz_favorites")]
struct QobuzFavorite {
    #[welds(primary_key)]
    id: i64,
    entity_type: String,
    qobuz_id: i64,
    title: Option<String>,
    artist_name: Option<String>,
    synced_at: String,
    removed: i64,
    slug: Option<String>,
    cover_url: Option<String>,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "albums")]
struct Album {
    #[welds(primary_key)]
    id: i64,
    artist_id: Option<i64>,
    title: String,
    year: Option<i32>,
    qobuz_album_id: Option<i64>,
    path: Option<String>,
    cover_path: Option<String>,
    created_at: String,
    updated_at: String,
}

pub async fn upsert_album(
    handle: &DataHandle,
    qobuz_id: u64,
    title: &str,
    artist_name: &str,
    album_api_id: Option<&str>,
    cover_url: Option<&str>,
) -> Result<bool> {
    let qobuz_id = qobuz_id as i64;
    let synced_at = Utc::now().to_rfc3339();
    if let Some(mut favorite) = QobuzFavorite::all()
        .run(handle.client())
        .await?
        .into_iter()
        .find(|favorite| favorite.entity_type == "album" && favorite.qobuz_id == qobuz_id)
    {
        favorite.title = Some(title.to_string());
        favorite.artist_name = Some(artist_name.to_string());
        favorite.slug = album_api_id.map(ToString::to_string);
        if cover_url.is_some() {
            favorite.cover_url = cover_url.map(ToString::to_string);
        }
        favorite.synced_at = synced_at;
        favorite.removed = 0;
        favorite.save(handle.client()).await?;
        return Ok(true);
    }

    let mut favorite = QobuzFavorite::new();
    favorite.entity_type = "album".to_string();
    favorite.qobuz_id = qobuz_id;
    favorite.title = Some(title.to_string());
    favorite.artist_name = Some(artist_name.to_string());
    favorite.synced_at = synced_at;
    favorite.removed = 0;
    favorite.slug = album_api_id.map(ToString::to_string);
    favorite.cover_url = cover_url.map(ToString::to_string);
    favorite.save(handle.client()).await?;
    Ok(true)
}

pub async fn album_meta(handle: &DataHandle, qobuz_id: u64) -> Result<Option<FavoriteAlbumMeta>> {
    Ok(QobuzFavorite::all()
        .run(handle.client())
        .await?
        .into_iter()
        .find(|favorite| {
            favorite.entity_type == "album"
                && favorite.qobuz_id == qobuz_id as i64
                && favorite.removed == 0
        })
        .map(|favorite| FavoriteAlbumMeta {
            slug: favorite.slug.clone().filter(|slug| !slug.trim().is_empty()),
            title: favorite.title.clone().unwrap_or_default(),
            artist_name: favorite.artist_name.clone().unwrap_or_default(),
        }))
}

pub async fn mark_removed_except(handle: &DataHandle, keep_ids: &[u64]) -> Result<u64> {
    let keep_ids = keep_ids.iter().map(|id| *id as i64).collect::<Vec<_>>();
    let mut removed = 0;
    for mut favorite in QobuzFavorite::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|favorite| favorite.entity_type == "album" && favorite.removed == 0)
    {
        if keep_ids.is_empty() || !keep_ids.contains(&favorite.qobuz_id) {
            favorite.removed = 1;
            favorite.save(handle.client()).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub async fn list_albums_keyset(
    handle: &DataHandle,
    params: FavoritesListParams,
) -> Result<QobuzFavoriteAlbumPage> {
    let albums_by_qobuz_id = Album::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter_map(|album| album.qobuz_album_id.map(|qobuz_id| (qobuz_id, album)))
        .collect::<HashMap<_, _>>();

    let query = params
        .q
        .as_deref()
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(str::to_lowercase);

    let mut rows = QobuzFavorite::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|favorite| favorite.entity_type == "album" && favorite.removed == 0)
        .map(|favorite| {
            let album = albums_by_qobuz_id.get(&favorite.qobuz_id);
            FavoriteJoinedRow {
                qobuz_id: favorite.qobuz_id,
                album_api_id: favorite.slug.clone(),
                title: favorite.title.clone(),
                artist_name: favorite.artist_name.clone(),
                cover_url: favorite.cover_url.clone(),
                local_album_id: album.map(|album| album.id),
                local_cover_path: album.and_then(|album| album.cover_path.clone()),
            }
        })
        .filter(|row| {
            params
                .in_library
                .is_none_or(|in_library| (row.local_album_id.is_some()) == in_library)
        })
        .filter(|row| {
            query.as_ref().is_none_or(|query| {
                row.title
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(query)
                    || row
                        .artist_name
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(query)
            })
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| compare_favorite_rows(left, right, params.sort, params.order));
    if let Some(after) = params.after.as_ref() {
        rows.retain(|row| favorite_row_is_after_cursor(row, params.sort, params.order, after));
    }

    let has_more = rows.len() > params.limit;
    rows.truncate(params.limit);
    let next_after = has_more
        .then(|| {
            rows.last()
                .map(|row| favorite_cursor_for_row(row, params.sort))
        })
        .flatten();

    Ok(QobuzFavoriteAlbumPage {
        items: rows.into_iter().map(favorite_row_to_item).collect(),
        next_after,
        has_more,
    })
}

pub async fn mark_albums_removed(handle: &DataHandle, ids: &[u64]) -> Result<()> {
    for mut favorite in QobuzFavorite::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|favorite| {
            favorite.entity_type == "album" && ids.contains(&(favorite.qobuz_id as u64))
        })
    {
        favorite.removed = 1;
        favorite.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn active_album_ids(handle: &DataHandle) -> Result<Vec<u64>> {
    let mut ids = QobuzFavorite::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|favorite| favorite.entity_type == "album" && favorite.removed == 0)
        .map(|favorite| favorite.qobuz_id as u64)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    Ok(ids)
}

fn favorite_row_to_item(row: FavoriteJoinedRow) -> QobuzFavoriteAlbum {
    QobuzFavoriteAlbum {
        album_api_id: row
            .album_api_id
            .filter(|slug| !slug.trim().is_empty())
            .unwrap_or_else(|| row.qobuz_id.to_string()),
        qobuz_id: row.qobuz_id,
        title: row.title.unwrap_or_default(),
        artist_name: row.artist_name.unwrap_or_default(),
        in_library: row.local_album_id.is_some(),
        local_album_id: row.local_album_id,
        local_cover_path: row.local_cover_path.filter(|path| !path.trim().is_empty()),
        cover_url: row.cover_url.filter(|url| !url.trim().is_empty()),
    }
}

fn compare_favorite_rows(
    left: &FavoriteJoinedRow,
    right: &FavoriteJoinedRow,
    sort: FavoritesSort,
    order: SortOrder,
) -> Ordering {
    let primary = compare_favorite_primary(left, right, sort);
    let ordered = match order {
        SortOrder::Asc => primary,
        SortOrder::Desc => primary.reverse(),
    };
    ordered.then_with(|| left.qobuz_id.cmp(&right.qobuz_id))
}

fn compare_favorite_primary(
    left: &FavoriteJoinedRow,
    right: &FavoriteJoinedRow,
    sort: FavoritesSort,
) -> Ordering {
    match sort {
        FavoritesSort::Title => {
            favorite_text_key(left.title.as_deref()).cmp(&favorite_text_key(right.title.as_deref()))
        }
        FavoritesSort::Artist => favorite_text_key(left.artist_name.as_deref())
            .cmp(&favorite_text_key(right.artist_name.as_deref())),
        FavoritesSort::InLibrary => favorite_bool_key(left).cmp(&favorite_bool_key(right)),
    }
}

fn favorite_row_is_after_cursor(
    row: &FavoriteJoinedRow,
    sort: FavoritesSort,
    order: SortOrder,
    cursor: &FavoriteListCursor,
) -> bool {
    let primary_cmp = compare_sort_value(&favorite_sort_value_for_row(row, sort), &cursor.primary);
    match order {
        SortOrder::Asc => {
            primary_cmp == Ordering::Greater
                || (primary_cmp == Ordering::Equal && row.qobuz_id > cursor.tie_qobuz_id)
        }
        SortOrder::Desc => {
            primary_cmp == Ordering::Less
                || (primary_cmp == Ordering::Equal && row.qobuz_id > cursor.tie_qobuz_id)
        }
    }
}

fn favorite_cursor_for_row(row: &FavoriteJoinedRow, sort: FavoritesSort) -> FavoriteListCursor {
    FavoriteListCursor {
        primary: favorite_sort_value_for_row(row, sort),
        tie_qobuz_id: row.qobuz_id,
    }
}

fn favorite_sort_value_for_row(row: &FavoriteJoinedRow, sort: FavoritesSort) -> FavoriteSortValue {
    match sort {
        FavoritesSort::Title => FavoriteSortValue::Text(row.title.clone().unwrap_or_default()),
        FavoritesSort::Artist => {
            FavoriteSortValue::Text(row.artist_name.clone().unwrap_or_default())
        }
        FavoritesSort::InLibrary => FavoriteSortValue::Bool(favorite_bool_key(row)),
    }
}

fn compare_sort_value(left: &FavoriteSortValue, right: &FavoriteSortValue) -> Ordering {
    match (left, right) {
        (FavoriteSortValue::Text(left), FavoriteSortValue::Text(right)) => {
            left.to_lowercase().cmp(&right.to_lowercase())
        }
        (FavoriteSortValue::Bool(left), FavoriteSortValue::Bool(right)) => left.cmp(right),
        (FavoriteSortValue::Text(_), FavoriteSortValue::Bool(_)) => Ordering::Less,
        (FavoriteSortValue::Bool(_), FavoriteSortValue::Text(_)) => Ordering::Greater,
    }
}

fn favorite_text_key(value: Option<&str>) -> String {
    value.unwrap_or_default().to_lowercase()
}

fn favorite_bool_key(row: &FavoriteJoinedRow) -> i32 {
    if row.local_album_id.is_some() { 1 } else { 0 }
}

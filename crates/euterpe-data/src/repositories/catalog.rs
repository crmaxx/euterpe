use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::connection::DataHandle;
use crate::error::{DataError, Result};
use welds::prelude::*;
use welds::query::builder::ManualParam;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumRow {
    pub id: i64,
    pub artist_id: Option<i64>,
    pub title: String,
    pub year: Option<i32>,
    pub qobuz_album_id: Option<i64>,
    pub path: Option<String>,
    pub cover_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRow {
    pub id: i64,
    pub album_id: i64,
    pub title: String,
    pub track_number: Option<i32>,
    pub year: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<String>,
    pub qobuz_track_id: Option<i64>,
    pub path: String,
    pub duration_sec: Option<i32>,
    pub file_mtime: Option<String>,
    pub file_hash: Option<String>,
    pub file_size: Option<i64>,
}

pub struct AlbumUpsert<'a> {
    pub artist_id: Option<i64>,
    pub title: &'a str,
    pub year: Option<i32>,
    pub qobuz_album_id: Option<i64>,
    pub path: Option<&'a str>,
    pub cover_path: Option<&'a str>,
}

pub struct TrackUpsert<'a> {
    pub album_id: i64,
    pub title: &'a str,
    pub track_number: Option<i32>,
    pub year: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<&'a str>,
    pub qobuz_track_id: Option<i64>,
    pub path: &'a str,
    pub duration_sec: Option<i32>,
    pub file_mtime: Option<&'a str>,
    pub file_hash: Option<&'a str>,
    pub file_size: Option<i64>,
}

pub struct TrackMetadataUpdate<'a> {
    pub title: &'a str,
    pub track_number: Option<i32>,
    pub year: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<&'a str>,
    pub file_mtime: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumListSort {
    Title,
    Artist,
    AlbumDate,
    DateAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumListOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlbumListSortValue {
    Text(String),
    Int(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumListCursor {
    pub primary: AlbumListSortValue,
    pub tie_id: i64,
}

#[derive(Debug, Clone)]
pub struct AlbumListParams {
    pub sort: AlbumListSort,
    pub order: AlbumListOrder,
    pub limit: usize,
    pub q: Option<String>,
    pub after: Option<AlbumListCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumListPage {
    pub items: Vec<AlbumListRow>,
    pub next_after: Option<AlbumListCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackHashBackfillRow {
    pub id: i64,
    pub path: String,
    pub file_size: Option<i64>,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "artists")]
#[welds(HasMany(albums, Album, "artist_id"))]
struct Artist {
    #[welds(primary_key)]
    id: i64,
    name: String,
    qobuz_artist_id: Option<i64>,
    created_at: String,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "albums")]
#[welds(BelongsTo(artist, Artist, "artist_id"))]
#[welds(HasMany(tracks, Track, "album_id"))]
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

#[derive(Debug, WeldsModel)]
#[welds(table = "tracks")]
#[welds(BelongsTo(album, Album, "album_id"))]
struct Track {
    #[welds(primary_key)]
    id: i64,
    album_id: i64,
    title: String,
    track_number: Option<i32>,
    year: Option<i32>,
    disc_number: Option<i32>,
    genre: Option<String>,
    qobuz_track_id: Option<i64>,
    path: String,
    duration_sec: Option<i32>,
    file_mtime: Option<String>,
    file_hash: Option<String>,
    file_size: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "scan_keep_paths")]
struct ScanKeepPath {
    #[welds(primary_key)]
    id: i64,
    scan_id: i64,
    path: String,
}

#[derive(Debug, WeldsModel)]
struct AlbumListPageRecord {
    id: i64,
    title: String,
    artist_name: String,
    year: Option<i32>,
    created_at: String,
    path: Option<String>,
    cover_path: Option<String>,
}

#[derive(Debug, WeldsModel)]
struct AlbumTrackCountRecord {
    album_id: i64,
    track_count: i64,
}

pub async fn upsert_artist_by_name(
    handle: &DataHandle,
    name: &str,
    qobuz_artist_id: Option<i64>,
) -> Result<i64> {
    let mut artists = Artist::all().run(handle.client()).await?;
    if let Some(qid) = qobuz_artist_id
        && let Some(existing) = artists
            .iter_mut()
            .find(|artist| artist.qobuz_artist_id == Some(qid))
    {
        existing.name = name.to_string();
        existing.save(handle.client()).await?;
        return Ok(existing.id);
    }

    if let Some(existing) = artists
        .iter_mut()
        .find(|artist| artist.qobuz_artist_id.is_none() && artist.name.eq_ignore_ascii_case(name))
    {
        if qobuz_artist_id.is_some() {
            existing.qobuz_artist_id = qobuz_artist_id;
            existing.save(handle.client()).await?;
        }
        return Ok(existing.id);
    }

    let now = sqlite_timestamp();
    let mut artist = Artist::new();
    artist.name = name.to_string();
    artist.qobuz_artist_id = qobuz_artist_id;
    artist.created_at = now;
    artist.save(handle.client()).await?;
    Ok(artist.id)
}

pub async fn get_artist_name_by_id(handle: &DataHandle, id: i64) -> Result<Option<String>> {
    Ok(Artist::find_by_id(handle.client(), id)
        .await?
        .map(|artist| artist.name.clone()))
}

pub async fn upsert_album(handle: &DataHandle, album: AlbumUpsert<'_>) -> Result<i64> {
    if let Some(id) = update_existing_album(handle, &album).await? {
        return Ok(id);
    }

    let now = sqlite_timestamp();
    let mut row = Album::new();
    row.artist_id = album.artist_id;
    row.title = album.title.to_string();
    row.year = album.year;
    row.qobuz_album_id = album.qobuz_album_id;
    row.path = album.path.map(ToString::to_string);
    row.cover_path = album.cover_path.map(ToString::to_string);
    row.created_at = now.clone();
    row.updated_at = now;
    if let Err(error) = row.save(handle.client()).await {
        if let Some(id) = update_existing_album(handle, &album).await? {
            return Ok(id);
        }
        return Err(DataError::from(error));
    }
    Ok(row.id)
}

async fn update_existing_album(
    handle: &DataHandle,
    album: &AlbumUpsert<'_>,
) -> Result<Option<i64>> {
    let mut albums = Album::all().run(handle.client()).await?;
    if let Some(path) = album.path
        && let Some(existing) = albums
            .iter_mut()
            .find(|existing| existing.path.as_deref() == Some(path))
    {
        apply_album_update(existing, album);
        existing.save(handle.client()).await?;
        return Ok(Some(existing.id));
    }

    if let Some(qid) = album.qobuz_album_id
        && let Some(existing) = albums
            .iter_mut()
            .find(|existing| existing.qobuz_album_id == Some(qid))
    {
        apply_album_update(existing, album);
        existing.save(handle.client()).await?;
        return Ok(Some(existing.id));
    }

    Ok(None)
}

pub async fn get_album_by_id(handle: &DataHandle, id: i64) -> Result<Option<AlbumRow>> {
    Ok(Album::find_by_id(handle.client(), id)
        .await?
        .map(|album| AlbumRow {
            id: album.id,
            artist_id: album.artist_id,
            title: album.title.clone(),
            year: album.year,
            qobuz_album_id: album.qobuz_album_id,
            path: album.path.clone(),
            cover_path: album.cover_path.clone(),
        }))
}

pub async fn list_albums_keyset(
    handle: &DataHandle,
    params: AlbumListParams,
) -> Result<AlbumListPage> {
    let query = normalize_album_search_query(params.q.clone());
    let page_records = query_album_list_page(handle, &params, query.as_deref()).await?;
    let has_more = page_records.len() > params.limit;
    let page_records = page_records
        .into_iter()
        .take(params.limit)
        .collect::<Vec<_>>();
    let track_counts = album_track_counts(handle, &page_records).await?;
    let rows = page_records
        .into_iter()
        .map(|album| AlbumListRow {
            id: album.id,
            title: album.title,
            artist_name: album.artist_name,
            year: album.year,
            created_at: album.created_at,
            path: album.path,
            cover_path: album.cover_path,
            track_count: track_counts.get(&album.id).copied().unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let next_after = has_more
        .then(|| {
            rows.last()
                .map(|row| album_cursor_for_row(row, params.sort, params.order))
        })
        .flatten();

    Ok(AlbumListPage {
        items: rows,
        next_after,
        has_more,
    })
}

async fn query_album_list_page(
    handle: &DataHandle,
    params: &AlbumListParams,
    query: Option<&str>,
) -> Result<Vec<AlbumListPageRecord>> {
    let mut query_builder = Album::all();
    if let Some(query) = query {
        let like = format!("%{query}%");
        query_builder = query_builder.where_manual2(
            "(LOWER($.title) LIKE ? OR LOWER(artist_name) LIKE ?)",
            ManualParam::new().with(like.clone()).with(like),
        );
    }
    if let Some(after) = params.after.as_ref() {
        let operator = match params.order {
            AlbumListOrder::Asc => ">",
            AlbumListOrder::Desc => "<",
        };
        let cursor_sql = album_list_cursor_sql(params.sort, params.order, operator);
        query_builder = query_builder.where_manual2(
            cursor_sql,
            album_cursor_params(&after.primary)
                .push_sort_value(&after.primary)
                .with(after.tie_id),
        );
    }

    Ok(query_builder
        .select_as(|album| album.id, "id")
        .select_as(|album| album.title, "title")
        .select_as(|album| album.year, "year")
        .select_as(|album| album.created_at, "created_at")
        .select_as(|album| album.path, "path")
        .select_as(|album| album.cover_path, "cover_path")
        .left_join(
            |album| album.artist,
            Artist::select_as(|artist| artist.name, "artist_name"),
        )
        .order_manual(album_list_order_sql(params.sort, params.order))
        .order_by_asc(|album| album.id)
        .limit(params.limit as i64 + 1)
        .run(handle.client())
        .await?
        .collect_into()?)
}

fn album_list_order_sql(sort: AlbumListSort, order: AlbumListOrder) -> &'static str {
    match sort {
        AlbumListSort::Title => match order {
            AlbumListOrder::Asc => "LOWER($.title) ASC",
            AlbumListOrder::Desc => "LOWER($.title) DESC",
        },
        AlbumListSort::Artist => match order {
            AlbumListOrder::Asc => "LOWER(artist_name) ASC",
            AlbumListOrder::Desc => "LOWER(artist_name) DESC",
        },
        AlbumListSort::AlbumDate => match order {
            AlbumListOrder::Asc => "CASE WHEN $.year IS NULL THEN 1 ELSE 0 END ASC, $.year ASC",
            AlbumListOrder::Desc => "CASE WHEN $.year IS NULL THEN 1 ELSE 0 END ASC, $.year DESC",
        },
        AlbumListSort::DateAdded => match order {
            AlbumListOrder::Asc => "$.created_at ASC",
            AlbumListOrder::Desc => "$.created_at DESC",
        },
    }
}

fn album_list_cursor_sql(
    sort: AlbumListSort,
    order: AlbumListOrder,
    operator: &'static str,
) -> &'static str {
    match (sort, order, operator) {
        (AlbumListSort::Title, AlbumListOrder::Asc, ">") => {
            "(LOWER($.title) > ? OR (LOWER($.title) = ? AND $.id > ?))"
        }
        (AlbumListSort::Title, AlbumListOrder::Desc, "<") => {
            "(LOWER($.title) < ? OR (LOWER($.title) = ? AND $.id > ?))"
        }
        (AlbumListSort::Artist, AlbumListOrder::Asc, ">") => {
            "(LOWER(artist_name) > ? OR (LOWER(artist_name) = ? AND $.id > ?))"
        }
        (AlbumListSort::Artist, AlbumListOrder::Desc, "<") => {
            "(LOWER(artist_name) < ? OR (LOWER(artist_name) = ? AND $.id > ?))"
        }
        (AlbumListSort::AlbumDate, AlbumListOrder::Asc, ">") => {
            "(CASE WHEN $.year IS NULL THEN 9223372036854775807 ELSE $.year END > ? OR (CASE WHEN $.year IS NULL THEN 9223372036854775807 ELSE $.year END = ? AND $.id > ?))"
        }
        (AlbumListSort::AlbumDate, AlbumListOrder::Desc, "<") => {
            "(CASE WHEN $.year IS NULL THEN -9223372036854775808 ELSE $.year END < ? OR (CASE WHEN $.year IS NULL THEN -9223372036854775808 ELSE $.year END = ? AND $.id > ?))"
        }
        (AlbumListSort::DateAdded, AlbumListOrder::Asc, ">") => {
            "($.created_at > ? OR ($.created_at = ? AND $.id > ?))"
        }
        (AlbumListSort::DateAdded, AlbumListOrder::Desc, "<") => {
            "($.created_at < ? OR ($.created_at = ? AND $.id > ?))"
        }
        _ => unreachable!("cursor operator must match sort order"),
    }
}

fn album_cursor_params(value: &AlbumListSortValue) -> ManualParam {
    ManualParam::new().push_sort_value(value)
}

trait AlbumCursorParams {
    fn push_sort_value(self, value: &AlbumListSortValue) -> Self;
}

impl AlbumCursorParams for ManualParam {
    fn push_sort_value(self, value: &AlbumListSortValue) -> Self {
        match value {
            AlbumListSortValue::Text(value) => self.with(value.to_lowercase()),
            AlbumListSortValue::Int(value) => self.with(*value),
        }
    }
}

async fn album_track_counts(
    handle: &DataHandle,
    albums: &[AlbumListPageRecord],
) -> Result<HashMap<i64, i64>> {
    if albums.is_empty() {
        return Ok(HashMap::new());
    }
    let album_ids = albums.iter().map(|album| album.id).collect::<Vec<_>>();
    Ok(Track::all()
        .where_col(|track| track.album_id.in_list(&album_ids))
        .select_as(|track| track.album_id, "album_id")
        .select_count(|track| track.id, "track_count")
        .group_by(|track| track.album_id)
        .run(handle.client())
        .await?
        .collect_into()?
        .into_iter()
        .map(|row: AlbumTrackCountRecord| (row.album_id, row.track_count))
        .collect())
}

pub fn normalize_album_search_query(q: Option<String>) -> Option<String> {
    q.as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(|q| q.to_lowercase())
}

pub async fn set_album_cover_path(handle: &DataHandle, id: i64, cover_path: &str) -> Result<bool> {
    let Some(mut album) = Album::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    album.cover_path = Some(cover_path.to_string());
    album.updated_at = sqlite_timestamp();
    album.save(handle.client()).await?;
    Ok(true)
}

pub async fn album_id_by_path(handle: &DataHandle, path: &str) -> Result<Option<i64>> {
    Ok(Album::all()
        .run(handle.client())
        .await?
        .into_iter()
        .find(|album| album.path.as_deref() == Some(path))
        .map(|album| album.id))
}

pub async fn album_id_by_path_or_prefix(handle: &DataHandle, path: &str) -> Result<Option<i64>> {
    let prefix = format!("{}/", path.trim_end_matches('/'));
    let mut rows = Album::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|album| {
            album
                .path
                .as_deref()
                .is_some_and(|album_path| album_path == path || album_path.starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|album| {
        (
            album.path.as_ref().map_or(usize::MAX, String::len),
            album.id,
        )
    });
    Ok(rows.first().map(|album| album.id))
}

pub async fn find_album_id_by_qobuz_album_id(
    handle: &DataHandle,
    qobuz_id: i64,
) -> Result<Option<i64>> {
    Ok(Album::all()
        .run(handle.client())
        .await?
        .into_iter()
        .find(|album| album.qobuz_album_id == Some(qobuz_id))
        .map(|album| album.id))
}

pub async fn delete_empty_storage_albums_in_scope(
    handle: &DataHandle,
    scope_path: Option<&str>,
) -> Result<u64> {
    let scope = normalized_scope(scope_path);
    let tracks = Track::all().run(handle.client()).await?;
    let mut deleted = 0;
    for mut album in Album::all().run(handle.client()).await? {
        let Some(path) = album.path.as_deref() else {
            continue;
        };
        if !scope.is_empty() && !path_in_scope(path, &scope) {
            continue;
        }
        if tracks.iter().any(|track| track.album_id == album.id) {
            continue;
        }
        album.delete(handle.client()).await?;
        deleted += 1;
    }
    Ok(deleted)
}

pub async fn upsert_track(handle: &DataHandle, track: TrackUpsert<'_>) -> Result<i64> {
    let mut tracks = Track::all().run(handle.client()).await?;
    if let Some(existing) = tracks
        .iter_mut()
        .find(|existing| existing.path == track.path)
    {
        apply_track_update(existing, &track);
        existing.save(handle.client()).await?;
        return Ok(existing.id);
    }

    let now = sqlite_timestamp();
    let mut row = Track::new();
    row.album_id = track.album_id;
    row.title = track.title.to_string();
    row.track_number = track.track_number;
    row.year = track.year;
    row.disc_number = track.disc_number;
    row.genre = track.genre.map(ToString::to_string);
    row.qobuz_track_id = track.qobuz_track_id;
    row.path = track.path.to_string();
    row.duration_sec = track.duration_sec;
    row.file_mtime = track.file_mtime.map(ToString::to_string);
    row.file_hash = track.file_hash.map(ToString::to_string);
    row.file_size = track.file_size;
    row.created_at = now.clone();
    row.updated_at = now;
    row.save(handle.client()).await?;
    Ok(row.id)
}

pub async fn get_track_by_id(handle: &DataHandle, id: i64) -> Result<Option<TrackRow>> {
    Ok(Track::find_by_id(handle.client(), id)
        .await?
        .map(track_row_from_model))
}

pub async fn list_tracks_by_album(handle: &DataHandle, album_id: i64) -> Result<Vec<TrackRow>> {
    let mut rows = Track::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|track| track.album_id == album_id)
        .map(track_row_from_model)
        .collect::<Vec<_>>();
    rows.sort_by_key(|track| (filename_sort_key(&track.path), track.path.clone()));
    Ok(rows)
}

pub async fn list_tracks_by_album_or_path_prefix(
    handle: &DataHandle,
    album_id: i64,
    album_path: Option<&str>,
) -> Result<Vec<TrackRow>> {
    let Some(album_path) = album_path.map(str::trim).filter(|path| !path.is_empty()) else {
        return list_tracks_by_album(handle, album_id).await;
    };
    let mut rows = Track::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|track| track.album_id == album_id || path_in_scope(&track.path, album_path))
        .map(track_row_from_model)
        .collect::<Vec<_>>();
    rows.sort_by_key(|track| (filename_sort_key(&track.path), track.path.clone()));
    Ok(rows)
}

pub async fn get_track_fingerprint_by_path(
    handle: &DataHandle,
    path: &str,
) -> Result<Option<(Option<String>, Option<i64>)>> {
    Ok(Track::all()
        .run(handle.client())
        .await?
        .into_iter()
        .find(|track| track.path == path)
        .map(|track| (track.file_mtime.clone(), track.file_size)))
}

pub async fn update_track_metadata(
    handle: &DataHandle,
    id: i64,
    meta: TrackMetadataUpdate<'_>,
) -> Result<bool> {
    let Some(mut track) = Track::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    track.title = meta.title.to_string();
    track.track_number = meta.track_number;
    track.year = meta.year;
    track.disc_number = meta.disc_number;
    track.genre = meta.genre.map(ToString::to_string);
    track.file_mtime = meta.file_mtime.map(ToString::to_string);
    track.updated_at = sqlite_timestamp();
    track.save(handle.client()).await?;
    Ok(true)
}

pub async fn update_track_path(handle: &DataHandle, id: i64, path: &str) -> Result<bool> {
    let Some(mut track) = Track::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    track.path = path.to_string();
    track.updated_at = sqlite_timestamp();
    track.save(handle.client()).await?;
    Ok(true)
}

pub async fn delete_track_by_path(handle: &DataHandle, path: &str) -> Result<u64> {
    let mut deleted = 0;
    for mut track in Track::all().run(handle.client()).await? {
        if track.path == path {
            track.delete(handle.client()).await?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

pub async fn delete_tracks_by_path_or_prefix(handle: &DataHandle, path: &str) -> Result<u64> {
    let prefix = format!("{}/", path.trim_end_matches('/'));
    let mut deleted = 0;
    for mut track in Track::all().run(handle.client()).await? {
        if track.path == path || track.path.starts_with(&prefix) {
            track.delete(handle.client()).await?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

pub async fn delete_absent_in_scope(
    handle: &DataHandle,
    scope_path: Option<&str>,
    keep_paths: &[String],
) -> Result<u64> {
    let keep = keep_paths
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    delete_tracks_missing_from_scope_keep_set(handle, scope_path, &keep).await
}

pub async fn reset_scan_keep_paths(handle: &DataHandle, scan_id: i64) -> Result<()> {
    for mut keep in ScanKeepPath::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|keep| keep.scan_id == scan_id)
    {
        keep.delete(handle.client()).await?;
    }
    Ok(())
}

pub async fn record_scan_keep_path(handle: &DataHandle, scan_id: i64, path: &str) -> Result<()> {
    let exists = ScanKeepPath::all()
        .run(handle.client())
        .await?
        .into_iter()
        .any(|keep| keep.scan_id == scan_id && keep.path == path);
    if exists {
        return Ok(());
    }

    let mut keep = ScanKeepPath::new();
    keep.scan_id = scan_id;
    keep.path = path.to_string();
    keep.save(handle.client()).await?;
    Ok(())
}

pub async fn delete_absent_in_scope_for_scan(
    handle: &DataHandle,
    scope_path: Option<&str>,
    scan_id: i64,
) -> Result<u64> {
    let keep = ScanKeepPath::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|keep| keep.scan_id == scan_id)
        .map(|keep| keep.path.clone())
        .collect::<HashSet<_>>();
    let keep_refs = keep.iter().map(String::as_str).collect::<HashSet<_>>();
    delete_tracks_missing_from_scope_keep_set(handle, scope_path, &keep_refs).await
}

pub async fn cleanup_scan_keep_paths(handle: &DataHandle, scan_id: i64) -> Result<()> {
    reset_scan_keep_paths(handle, scan_id).await
}

pub async fn scan_keep_path_count(handle: &DataHandle, scan_id: i64) -> Result<usize> {
    Ok(ScanKeepPath::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|keep| keep.scan_id == scan_id)
        .count())
}

pub async fn list_tracks_needing_file_hash_batch(
    handle: &DataHandle,
    after_id: i64,
    limit: i64,
) -> Result<Vec<TrackHashBackfillRow>> {
    let mut rows = Track::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|track| {
            track.id > after_id
                && track
                    .file_hash
                    .as_deref()
                    .is_none_or(|hash| hash.trim().is_empty())
        })
        .map(|track| TrackHashBackfillRow {
            id: track.id,
            path: track.path.clone(),
            file_size: track.file_size,
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.id);
    rows.truncate(limit.max(0) as usize);
    Ok(rows)
}

pub async fn set_track_file_hash(handle: &DataHandle, id: i64, file_hash: &str) -> Result<bool> {
    let Some(mut track) = Track::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    track.file_hash = Some(file_hash.to_string());
    track.updated_at = sqlite_timestamp();
    track.save(handle.client()).await?;
    Ok(true)
}

pub async fn set_track_file_size(handle: &DataHandle, id: i64, file_size: i64) -> Result<bool> {
    let Some(mut track) = Track::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    track.file_size = Some(file_size);
    track.updated_at = sqlite_timestamp();
    track.save(handle.client()).await?;
    Ok(true)
}

pub async fn count_tracks(handle: &DataHandle) -> Result<usize> {
    Ok(Track::all().run(handle.client()).await?.len())
}

pub async fn count_distinct_track_paths(handle: &DataHandle) -> Result<usize> {
    Ok(Track::all()
        .run(handle.client())
        .await?
        .into_iter()
        .map(|track| track.path.clone())
        .collect::<HashSet<_>>()
        .len())
}

pub async fn update_track_path_fingerprint(
    handle: &DataHandle,
    id: i64,
    path: &str,
    file_size: Option<i64>,
    file_hash: Option<&str>,
    file_mtime: Option<&str>,
) -> Result<bool> {
    let Some(mut track) = Track::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    track.path = path.to_string();
    track.file_size = file_size;
    track.file_hash = file_hash.map(ToString::to_string);
    track.file_mtime = file_mtime.map(ToString::to_string);
    track.updated_at = sqlite_timestamp();
    track.save(handle.client()).await?;
    Ok(true)
}

fn apply_album_update(album: &mut Album, update: &AlbumUpsert<'_>) {
    album.artist_id = update.artist_id;
    album.title = update.title.to_string();
    album.year = update.year;
    if update.qobuz_album_id.is_some() {
        album.qobuz_album_id = update.qobuz_album_id;
    }
    if update.path.is_some() {
        album.path = update.path.map(ToString::to_string);
    }
    if update.cover_path.is_some() {
        album.cover_path = update.cover_path.map(ToString::to_string);
    }
    album.updated_at = sqlite_timestamp();
}

fn apply_track_update(track: &mut Track, update: &TrackUpsert<'_>) {
    track.album_id = update.album_id;
    track.title = update.title.to_string();
    track.track_number = update.track_number;
    track.year = update.year;
    track.disc_number = update.disc_number;
    track.genre = update.genre.map(ToString::to_string);
    if update.qobuz_track_id.is_some() {
        track.qobuz_track_id = update.qobuz_track_id;
    }
    track.duration_sec = update.duration_sec;
    track.file_mtime = update.file_mtime.map(ToString::to_string);
    track.file_hash = update.file_hash.map(ToString::to_string);
    track.file_size = update.file_size;
    track.updated_at = sqlite_timestamp();
}

fn track_row_from_model(track: welds::state::DbState<Track>) -> TrackRow {
    TrackRow {
        id: track.id,
        album_id: track.album_id,
        title: track.title.clone(),
        track_number: track.track_number,
        year: track.year,
        disc_number: track.disc_number,
        genre: track.genre.clone(),
        qobuz_track_id: track.qobuz_track_id,
        path: track.path.clone(),
        duration_sec: track.duration_sec,
        file_mtime: track.file_mtime.clone(),
        file_hash: track.file_hash.clone(),
        file_size: track.file_size,
    }
}

async fn delete_tracks_missing_from_scope_keep_set(
    handle: &DataHandle,
    scope_path: Option<&str>,
    keep_paths: &HashSet<&str>,
) -> Result<u64> {
    let scope = normalized_scope(scope_path);
    let mut deleted = 0;
    for mut track in Track::all().run(handle.client()).await? {
        if path_in_scope(&track.path, &scope) && !keep_paths.contains(track.path.as_str()) {
            track.delete(handle.client()).await?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

pub fn album_date_sort_value(year: Option<i32>, order: AlbumListOrder) -> i64 {
    match (year, order) {
        (Some(year), _) => i64::from(year),
        (None, AlbumListOrder::Asc) => i64::MAX,
        (None, AlbumListOrder::Desc) => i64::MIN,
    }
}

fn album_cursor_for_row(
    row: &AlbumListRow,
    sort: AlbumListSort,
    order: AlbumListOrder,
) -> AlbumListCursor {
    AlbumListCursor {
        primary: album_sort_value_for_row(row, sort, order),
        tie_id: row.id,
    }
}

fn album_sort_value_for_row(
    row: &AlbumListRow,
    sort: AlbumListSort,
    order: AlbumListOrder,
) -> AlbumListSortValue {
    match sort {
        AlbumListSort::Title => AlbumListSortValue::Text(row.title.clone()),
        AlbumListSort::Artist => AlbumListSortValue::Text(row.artist_name.clone()),
        AlbumListSort::AlbumDate => AlbumListSortValue::Int(album_date_sort_value(row.year, order)),
        AlbumListSort::DateAdded => AlbumListSortValue::Text(row.created_at.clone()),
    }
}

fn filename_sort_key(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| path.to_lowercase())
}

fn normalized_scope(scope_path: Option<&str>) -> String {
    scope_path
        .unwrap_or_default()
        .trim()
        .trim_matches('/')
        .to_string()
}

fn path_in_scope(path: &str, scope: &str) -> bool {
    if scope.is_empty() {
        return true;
    }
    let prefix = format!("{}/", scope.trim_end_matches('/'));
    path == scope || path.starts_with(&prefix)
}

fn sqlite_timestamp() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

use std::path::Path;

use crate::connection::DataHandle;
use crate::error::Result;
use welds::WeldsModel;

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

#[derive(Debug, WeldsModel)]
#[welds(table = "artists")]
struct Artist {
    #[welds(primary_key)]
    id: i64,
    name: String,
    qobuz_artist_id: Option<i64>,
    created_at: String,
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

#[derive(Debug, WeldsModel)]
#[welds(table = "tracks")]
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

pub async fn upsert_album(handle: &DataHandle, album: AlbumUpsert<'_>) -> Result<i64> {
    let mut albums = Album::all().run(handle.client()).await?;
    if let Some(path) = album.path
        && let Some(existing) = albums
            .iter_mut()
            .find(|existing| existing.path.as_deref() == Some(path))
    {
        apply_album_update(existing, &album);
        existing.save(handle.client()).await?;
        return Ok(existing.id);
    }

    if let Some(qid) = album.qobuz_album_id
        && let Some(existing) = albums
            .iter_mut()
            .find(|existing| existing.qobuz_album_id == Some(qid))
    {
        apply_album_update(existing, &album);
        existing.save(handle.client()).await?;
        return Ok(existing.id);
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
    row.save(handle.client()).await?;
    Ok(row.id)
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

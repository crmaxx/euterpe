use crate::connection::DataHandle;
use crate::error::Result;
use crate::repositories::catalog::{self, AlbumRow, AlbumUpsert, TrackRow, TrackUpsert};

#[derive(Debug, Clone)]
pub struct ArtistFixture {
    pub name: String,
    pub qobuz_artist_id: Option<i64>,
}

impl Default for ArtistFixture {
    fn default() -> Self {
        Self {
            name: "Artist".to_string(),
            qobuz_artist_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlbumFixture {
    pub artist: ArtistFixture,
    pub title: String,
    pub year: Option<i32>,
    pub qobuz_album_id: Option<i64>,
    pub path: Option<String>,
    pub cover_path: Option<String>,
}

impl Default for AlbumFixture {
    fn default() -> Self {
        Self {
            artist: ArtistFixture::default(),
            title: "Album".to_string(),
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Album".to_string()),
            cover_path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrackFixture {
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

impl TrackFixture {
    pub fn for_album(album_id: i64, path: impl Into<String>) -> Self {
        Self {
            album_id,
            title: "Track".to_string(),
            track_number: None,
            year: None,
            disc_number: None,
            genre: None,
            qobuz_track_id: None,
            path: path.into(),
            duration_sec: None,
            file_mtime: None,
            file_hash: None,
            file_size: None,
        }
    }
}

pub async fn seed_artist(handle: &DataHandle, fixture: ArtistFixture) -> Result<i64> {
    catalog::upsert_artist_by_name(handle, &fixture.name, fixture.qobuz_artist_id).await
}

pub async fn seed_album(handle: &DataHandle, fixture: AlbumFixture) -> Result<i64> {
    let artist_id = seed_artist(handle, fixture.artist).await?;
    catalog::upsert_album(
        handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: &fixture.title,
            year: fixture.year,
            qobuz_album_id: fixture.qobuz_album_id,
            path: fixture.path.as_deref(),
            cover_path: fixture.cover_path.as_deref(),
        },
    )
    .await
}

pub async fn seed_track(handle: &DataHandle, fixture: TrackFixture) -> Result<i64> {
    catalog::upsert_track(
        handle,
        TrackUpsert {
            album_id: fixture.album_id,
            title: &fixture.title,
            track_number: fixture.track_number,
            year: fixture.year,
            disc_number: fixture.disc_number,
            genre: fixture.genre.as_deref(),
            qobuz_track_id: fixture.qobuz_track_id,
            path: &fixture.path,
            duration_sec: fixture.duration_sec,
            file_mtime: fixture.file_mtime.as_deref(),
            file_hash: fixture.file_hash.as_deref(),
            file_size: fixture.file_size,
        },
    )
    .await
}

pub async fn seed_album_with_track(
    handle: &DataHandle,
    album: AlbumFixture,
    track: impl FnOnce(i64) -> TrackFixture,
) -> Result<(i64, i64)> {
    let album_id = seed_album(handle, album).await?;
    let track_id = seed_track(handle, track(album_id)).await?;
    Ok((album_id, track_id))
}

pub async fn album(handle: &DataHandle, album_id: i64) -> Result<Option<AlbumRow>> {
    catalog::get_album_by_id(handle, album_id).await
}

pub async fn track(handle: &DataHandle, track_id: i64) -> Result<Option<TrackRow>> {
    catalog::get_track_by_id(handle, track_id).await
}

pub async fn tracks_for_album(handle: &DataHandle, album_id: i64) -> Result<Vec<TrackRow>> {
    catalog::list_tracks_by_album(handle, album_id).await
}

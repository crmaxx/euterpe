//! After a Qobuz album download, upsert `albums` with `qobuz_album_id` = `download_jobs.qobuz_id`
//! (same as `qobuz_favorites.qobuz_id`) so favorites `in_library` matches without a full scan.

use std::path::Path;

use euterpe_qobuz::{AlbumDetail, Quality};
use sqlx::SqlitePool;

use crate::db::{albums, artists};
use crate::error::ApiError;
use crate::library::paths::{track_path, year_from_release_date};

/// `favorite_catalog_id` must match `download_jobs.qobuz_id` / `qobuz_favorites.qobuz_id` for the
/// favorites list `LEFT JOIN albums ON albums.qobuz_album_id = qobuz_favorites.qobuz_id`.
pub async fn register_album_from_qobuz_download(
    pool: &SqlitePool,
    library_root: &Path,
    favorite_catalog_id: u64,
    album: &AlbumDetail,
    quality: Quality,
) -> Result<(), ApiError> {
    let Some(first_track) = album.tracks.as_ref().and_then(|t| t.items.first()) else {
        return Ok(());
    };
    let track_file = track_path(library_root, album, first_track, quality.format_id());
    let album_dir = track_file
        .parent()
        .ok_or_else(|| ApiError::Message("track has no parent dir".into()))?;
    let album_path_str = album_dir
        .strip_prefix(library_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|_| ApiError::Message("path outside library root".into()))?;

    let artist_name = album
        .summary
        .artist
        .as_ref()
        .map(|a| a.name.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Unknown Artist");
    let artist_id = artists::upsert_by_name(pool, artist_name, None).await?;

    let year = year_from_release_date(album.summary.release_date_original.as_deref());

    albums::upsert(
        pool,
        albums::AlbumUpsert {
            artist_id: Some(artist_id),
            title: &album.summary.title,
            year,
            qobuz_album_id: Some(favorite_catalog_id as i64),
            path: Some(&album_path_str),
            cover_path: None,
        },
    )
    .await?;

    Ok(())
}

//! After a Qobuz album download, upsert `albums` + `tracks` so Library UI works without `library/scan`.

use std::path::Path;

use euterpe_qobuz::{AlbumDetail, Quality, TrackSummary};
use sqlx::SqlitePool;

use crate::db::{albums, artists, tracks};
use crate::error::ApiError;
use crate::library::fs::file_stat_sync;
use crate::library::paths::track_relative_path;
use crate::library::paths::{track_path, year_from_release_date};
use crate::library::qobuz_tags::track_db_fields_from_qobuz;
use crate::library::storage::{LibraryStorage, StoragePath};

fn relative_path(library_root: &Path, absolute: &Path) -> Result<String, ApiError> {
    absolute
        .strip_prefix(library_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|_| ApiError::Message("path outside library root".into()))
}

/// `favorite_catalog_id` must match `download_jobs.qobuz_id` / `qobuz_favorites.qobuz_id` for the
/// favorites list `LEFT JOIN albums ON albums.qobuz_album_id = qobuz_favorites.qobuz_id`.
pub async fn register_album_from_qobuz_download(
    pool: &SqlitePool,
    library_root: &Path,
    favorite_catalog_id: u64,
    album: &AlbumDetail,
    quality: Quality,
) -> Result<(), ApiError> {
    let track_items = album
        .tracks
        .as_ref()
        .map(|t| t.items.as_slice())
        .unwrap_or(&[]);
    if track_items.is_empty() {
        return Ok(());
    }

    let first_track = &track_items[0];
    let track_file = track_path(library_root, album, first_track, quality.format_id());
    let album_dir = track_file
        .parent()
        .ok_or_else(|| ApiError::Message("track has no parent dir".into()))?;
    let album_path_str = relative_path(library_root, album_dir)?;

    let artist_name = album
        .summary
        .artist
        .as_ref()
        .map(|a| a.name.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Unknown Artist");
    let artist_id = artists::upsert_by_name(pool, artist_name, None).await?;

    let year = year_from_release_date(album.summary.release_date_original.as_deref());

    let album_id = albums::upsert(
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

    let format_id = quality.format_id();
    for track in track_items {
        upsert_track_from_api(pool, library_root, album_id, album, track, format_id, year).await?;
    }

    Ok(())
}

pub async fn register_album_from_qobuz_download_storage(
    pool: &SqlitePool,
    storage: &dyn LibraryStorage,
    favorite_catalog_id: u64,
    album: &AlbumDetail,
    quality: Quality,
) -> Result<(), ApiError> {
    let track_items = album
        .tracks
        .as_ref()
        .map(|t| t.items.as_slice())
        .unwrap_or(&[]);
    if track_items.is_empty() {
        return Ok(());
    }

    let first_track = &track_items[0];
    let first_rel = track_relative_path(album, first_track, quality.format_id());
    let album_path_str = StoragePath::parse(&first_rel)?
        .parent()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();

    let artist_name = album
        .summary
        .artist
        .as_ref()
        .map(|a| a.name.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Unknown Artist");
    let artist_id = artists::upsert_by_name(pool, artist_name, None).await?;
    let year = year_from_release_date(album.summary.release_date_original.as_deref());
    let album_id = albums::upsert(
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

    let format_id = quality.format_id();
    for track in track_items {
        let path_str = track_relative_path(album, track, format_id);
        let meta = storage.metadata(&StoragePath::parse(&path_str)?).await.ok();
        let file_size = meta.and_then(|m| i64::try_from(m.size).ok());
        let (disc_number, genre) = track_db_fields_from_qobuz(album, track);
        tracks::upsert(
            pool,
            tracks::TrackUpsert {
                album_id,
                title: &track.title,
                track_number: track.track_number.map(|n| n as i32),
                year,
                disc_number,
                genre: genre.as_deref(),
                qobuz_track_id: Some(track.id as i64),
                path: &path_str,
                duration_sec: track.duration.map(|d| d as i32),
                file_mtime: None,
                file_hash: None,
                file_size,
            },
        )
        .await?;
    }
    Ok(())
}

async fn upsert_track_from_api(
    pool: &SqlitePool,
    library_root: &Path,
    album_id: i64,
    album: &AlbumDetail,
    track: &TrackSummary,
    format_id: u8,
    album_year: Option<i32>,
) -> Result<(), ApiError> {
    let dest = track_path(library_root, album, track, format_id);
    let path_str = relative_path(library_root, &dest)?;
    let dest_for_stat = dest.clone();
    let (mtime, size) = tokio::task::spawn_blocking(move || file_stat_sync(&dest_for_stat))
        .await
        .map_err(|e| ApiError::Message(format!("stat task join: {e}")))?;
    let file_size = i64::try_from(size).ok();
    let (disc_number, genre) = track_db_fields_from_qobuz(album, track);

    tracks::upsert(
        pool,
        tracks::TrackUpsert {
            album_id,
            title: &track.title,
            track_number: track.track_number.map(|n| n as i32),
            year: album_year,
            disc_number,
            genre: genre.as_deref(),
            qobuz_track_id: Some(track.id as i64),
            path: &path_str,
            duration_sec: track.duration.map(|d| d as i32),
            file_mtime: mtime.as_deref(),
            file_hash: None,
            file_size,
        },
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use euterpe_qobuz::{AlbumDetail, AlbumSummary, AlbumTracks, ArtistRef, TrackSummary};
    use tempfile::tempdir;

    use super::*;
    use crate::db::{self, tracks};

    fn sample_album() -> AlbumDetail {
        AlbumDetail {
            summary: AlbumSummary {
                id: 100,
                qobuz_id: None,
                title: "Test Album".into(),
                artist: Some(ArtistRef {
                    id: 1,
                    name: "Artist".into(),
                }),
                artists: None,
                image: None,
                release_date_original: Some("2020-01-15".into()),
                hires: None,
                album_ref: None,
                slug: None,
                list_id: None,
                product_id: None,
                genre: None,
                label: None,
            },
            tracks: Some(AlbumTracks {
                items: vec![
                    TrackSummary {
                        id: 1001,
                        title: "One".into(),
                        track_number: Some(1),
                        duration: Some(200),
                        performer: None,
                        hires_streamable: None,
                        media_number: None,
                        genre: None,
                        isrc: None,
                        composer: None,
                    },
                    TrackSummary {
                        id: 1002,
                        title: "Two".into(),
                        track_number: Some(2),
                        duration: Some(180),
                        performer: None,
                        hires_streamable: None,
                        media_number: None,
                        genre: None,
                        isrc: None,
                        composer: None,
                    },
                ],
            }),
            description: None,
        }
    }

    #[tokio::test]
    async fn registers_album_and_all_tracks() {
        let dir = tempdir().unwrap();
        let album = sample_album();
        let quality = Quality::FlacCd;
        for track in &album.tracks.as_ref().unwrap().items {
            let path = track_path(dir.path(), &album, track, quality.format_id());
            tokio::fs::create_dir_all(path.parent().unwrap())
                .await
                .unwrap();
            tokio::fs::write(&path, b"audio").await.unwrap();
        }

        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();

        register_album_from_qobuz_download(&pool, dir.path(), 99, &album, quality)
            .await
            .unwrap();

        let rows = tracks::list_by_album(&pool, 1).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "One");
        assert_eq!(rows[0].qobuz_track_id, Some(1001));
        assert_eq!(rows[0].track_number, Some(1));
        assert_eq!(rows[0].year, Some(2020));
        assert!(rows[0].path.contains("2020 - Test Album"));
        assert!(rows[0].path.ends_with("01 - One.flac"));
        assert_eq!(rows[1].qobuz_track_id, Some(1002));
        assert!(rows[1].file_mtime.is_some());
    }

    #[tokio::test]
    async fn registers_track_when_file_already_on_disk() {
        let dir = tempdir().unwrap();
        let album = sample_album();
        let quality = Quality::FlacCd;
        let track = &album.tracks.as_ref().unwrap().items[0];
        let path = track_path(dir.path(), &album, track, quality.format_id());
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"existing").await.unwrap();

        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();

        register_album_from_qobuz_download(&pool, dir.path(), 42, &album, quality)
            .await
            .unwrap();

        let rows = tracks::list_by_album(&pool, 1).await.unwrap();
        assert_eq!(rows.len(), 2);
        let first = rows
            .iter()
            .find(|r| r.qobuz_track_id == Some(1001))
            .unwrap();
        assert_eq!(first.title, "One");
        assert!(dir.path().join(&first.path).is_file());
    }
}

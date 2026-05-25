use std::path::Path;
use std::sync::Arc;

use reqwest::Client;
use sqlx::SqlitePool;

use crate::db::{albums, artists, tracks};
use crate::error::ApiError;
use crate::integrations::types::{AlbumLookupContext, AlbumLookupTrack, AlbumMetadataRelease};
use crate::library::covers;
use crate::library::paths::library_path_hints;
use crate::library::storage::{LibraryStorage, StorageEntryKind, StoragePath};
use crate::library::tags::{self, TrackTagsPatch, apply_patch};

fn path_hints_for_album(
    album_path: Option<&str>,
    track_paths: &[String],
) -> Option<crate::library::paths::LibraryPathHints> {
    if let Some(ap) = album_path.filter(|s| !s.trim().is_empty())
        && let Some(first) = track_paths.first()
        && let Some(name) = Path::new(first).file_name().and_then(|n| n.to_str())
    {
        let synthetic = format!("{ap}/{name}");
        if let Some(h) = library_path_hints(&synthetic) {
            return Some(h);
        }
    }
    for rel in track_paths {
        if let Some(h) = library_path_hints(rel) {
            return Some(h);
        }
    }
    None
}

pub async fn build_lookup_context(
    pool: &SqlitePool,
    album_id: i64,
) -> Result<AlbumLookupContext, ApiError> {
    let album = albums::get_by_id(pool, album_id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let db_artist = if let Some(aid) = album.artist_id {
        sqlx::query_as::<_, (String,)>("SELECT name FROM artists WHERE id = ?")
            .bind(aid)
            .fetch_optional(pool)
            .await?
            .map(|(n,)| n)
            .unwrap_or_default()
    } else {
        String::new()
    };
    let track_rows = tracks::list_by_album(pool, album_id).await?;
    let track_paths: Vec<String> = track_rows.iter().map(|t| t.path.clone()).collect();
    let path_hints = path_hints_for_album(album.path.as_deref(), &track_paths);

    let (artist_name, album_title, year) = if let Some(h) = path_hints {
        (h.artist_name, h.album_title, h.year.or(album.year))
    } else {
        (db_artist, album.title.clone(), album.year)
    };

    let lookup_tracks: Vec<AlbumLookupTrack> = track_rows
        .iter()
        .map(|t| {
            let from_path = library_path_hints(&t.path);
            AlbumLookupTrack {
                title: from_path
                    .as_ref()
                    .and_then(|h| h.track_title.clone())
                    .unwrap_or_else(|| t.title.clone()),
                track_number: from_path
                    .and_then(|h| h.track_number.map(|n| n as i32))
                    .or(t.track_number),
                duration_sec: t.duration_sec,
            }
        })
        .collect();

    Ok(AlbumLookupContext {
        artist_name,
        album_title,
        year,
        tracks: lookup_tracks,
    })
}

pub struct ApplyAlbumMetadataResult {
    pub tracks_updated: u32,
    pub cover_applied: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub struct ApplyStorageDeps {
    pub storage: Arc<dyn LibraryStorage>,
}

pub async fn apply_release_to_album(
    deps: &ApplyStorageDeps,
    pool: &SqlitePool,
    http: &Client,
    album_id: i64,
    release: &AlbumMetadataRelease,
) -> Result<ApplyAlbumMetadataResult, ApiError> {
    let album = albums::get_by_id(pool, album_id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let album_rel = album
        .path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad_request("album has no directory path on disk"))?;
    let album_storage_path = StoragePath::parse(album_rel)?;
    let album_meta = deps
        .storage
        .metadata(&album_storage_path)
        .await
        .map_err(|_| ApiError::bad_request("album directory not found on disk"))?;
    if album_meta.kind != StorageEntryKind::Directory {
        return Err(ApiError::bad_request("album directory not found on disk"));
    }

    let track_rows = tracks::list_by_album(pool, album_id).await?;
    let mut tracks_updated = 0u32;
    let mut warnings = Vec::new();

    for db_track in &track_rows {
        let path_hints = library_path_hints(&db_track.path);
        let match_number = path_hints
            .as_ref()
            .and_then(|h| h.track_number.map(|n| n as i32))
            .or(db_track.track_number);
        let match_title = path_hints
            .and_then(|h| h.track_title)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| db_track.title.clone());

        let meta = match match_track(&release.tracks, match_number, &match_title) {
            Some(m) => m,
            None => {
                warnings.push(format!(
                    "no metadata match for track #{} ({})",
                    match_number.unwrap_or(0),
                    match_title
                ));
                continue;
            }
        };
        let track_path = existing_track_storage_path(deps.storage.as_ref(), &db_track.path).await?;
        let current = tags::read_tags_storage(deps.storage.as_ref(), &track_path).await?;
        let patch = TrackTagsPatch {
            title: Some(meta.title.clone()),
            artist: Some(release.artist_name.clone()),
            album: Some(release.title.clone()),
            track_number: meta.track_number,
            year: meta.year.or(release.year.map(|y| y as u32)),
            disc_number: meta.disc_number,
            genre: meta.genre.clone().or_else(|| release.genre.clone()),
        };
        let updated = apply_patch(&current, &patch);
        tags::write_tags_storage(deps.storage.as_ref(), &track_path, &updated).await?;
        tracks::update_metadata(
            pool,
            db_track.id,
            tracks::TrackMetadataUpdate {
                title: &meta.title,
                track_number: meta.track_number.map(|n| n as i32),
                year: meta.year.map(|y| y as i32).or(release.year),
                disc_number: meta.disc_number.map(|d| d as i32),
                genre: meta.genre.as_deref().or(release.genre.as_deref()),
                file_mtime: None,
            },
        )
        .await?;
        tracks_updated += 1;
    }

    let mut cover_applied = false;
    if let Some(ref url) = release.cover_url {
        match apply_cover(http, pool, deps.storage.as_ref(), album_rel, album_id, url).await {
            Ok(()) => cover_applied = true,
            Err(e) => warnings.push(format!("cover: {e}")),
        }
    }

    if release.artist_name != "unknown" && !release.artist_name.is_empty() {
        let artist_id = artists::upsert_by_name(pool, &release.artist_name, None).await?;
        let _ = albums::upsert(
            pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: &release.title,
                year: release.year,
                qobuz_album_id: album.qobuz_album_id,
                path: album.path.as_deref(),
                cover_path: album.cover_path.as_deref(),
            },
        )
        .await?;
    }

    Ok(ApplyAlbumMetadataResult {
        tracks_updated,
        cover_applied,
        warnings,
    })
}

async fn existing_track_storage_path(
    storage: &dyn LibraryStorage,
    track_rel: &str,
) -> Result<StoragePath, ApiError> {
    let path = StoragePath::parse(track_rel)?;
    let meta = storage.metadata(&path).await.map_err(|_| {
        ApiError::bad_request(format!("INTEGRATION_TRACK_FILE_NOT_FOUND:{track_rel}"))
    })?;
    if meta.kind != StorageEntryKind::File {
        return Err(ApiError::bad_request(format!(
            "INTEGRATION_TRACK_FILE_NOT_FOUND:{track_rel}"
        )));
    }
    Ok(path)
}

fn normalize_track_title(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix(|c: char| c.is_ascii_digit())
        .and_then(|rest| rest.strip_prefix(" - "))
        .map(str::trim)
        .unwrap_or(s);
    s.to_lowercase()
}

fn match_track<'a>(
    meta_tracks: &'a [crate::integrations::types::AlbumMetadataTrack],
    track_number: Option<i32>,
    title: &str,
) -> Option<&'a crate::integrations::types::AlbumMetadataTrack> {
    if meta_tracks.is_empty() {
        return None;
    }
    if let Some(n) = track_number.filter(|n| *n > 0) {
        if let Some(t) = meta_tracks
            .iter()
            .find(|t| t.track_number == Some(n as u32))
        {
            return Some(t);
        }
        if let Ok(idx) = usize::try_from(n)
            && (1..=meta_tracks.len()).contains(&idx)
        {
            return Some(&meta_tracks[idx - 1]);
        }
    }
    let title_l = normalize_track_title(title);
    if !title_l.is_empty() {
        return meta_tracks
            .iter()
            .find(|t| normalize_track_title(&t.title) == title_l);
    }
    None
}

async fn apply_cover(
    http: &Client,
    pool: &SqlitePool,
    storage: &dyn LibraryStorage,
    album_rel: &str,
    album_id: i64,
    url: &str,
) -> Result<(), ApiError> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?
        .error_for_status()
        .map_err(|e| ApiError::Message(e.to_string()))?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    covers::write_album_cover_from_bytes_storage(
        pool,
        storage,
        album_id,
        album_rel,
        bytes,
        content_type,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        ApplyStorageDeps, apply_release_to_album, existing_track_storage_path, match_track,
        path_hints_for_album,
    };
    use crate::db::{albums, tracks};
    use crate::integrations::types::{AlbumMetadataRelease, AlbumMetadataTrack};
    use crate::library::storage::{LocalStorage, StoragePath};
    use crate::library::tags::{self, TrackTags};

    fn meta_tracks() -> Vec<AlbumMetadataTrack> {
        vec![
            AlbumMetadataTrack {
                title: "Dare To Rebel".into(),
                track_number: Some(1),
                disc_number: Some(1),
                year: None,
                genre: None,
            },
            AlbumMetadataTrack {
                title: "Woven Spells of a Demon".into(),
                track_number: Some(2),
                disc_number: Some(1),
                year: None,
                genre: None,
            },
        ]
    }

    #[test]
    fn match_track_uses_path_number_when_db_tags_wrong() {
        let meta = meta_tracks();
        assert_eq!(
            match_track(&meta, Some(2), "Dare To Rebel").unwrap().title,
            "Woven Spells of a Demon"
        );
    }

    #[test]
    fn match_track_by_path_title_when_numbers_missing() {
        let meta = meta_tracks();
        assert_eq!(
            match_track(&meta, None, "woven spells of a demon")
                .unwrap()
                .title,
            "Woven Spells of a Demon"
        );
    }

    #[test]
    fn match_track_does_not_default_to_first() {
        let meta = meta_tracks();
        assert!(match_track(&meta, None, "").is_none());
    }

    #[test]
    fn path_hints_for_album_from_track_path() {
        let h = path_hints_for_album(
            None,
            &["Pg.lost/2009 - In Never Out/01 - Prahanien.flac".into()],
        )
        .unwrap();
        assert_eq!(h.artist_name, "Pg.lost");
        assert_eq!(h.album_title, "In Never Out");
        assert_eq!(h.year, Some(2009));
        assert_eq!(h.track_title.as_deref(), Some("Prahanien"));
    }

    #[tokio::test]
    async fn existing_track_storage_path_accepts_local_file() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(dir.path().join("Artist/Album"))
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("Artist/Album/01.flac"), b"audio")
            .await
            .unwrap();
        let storage = LocalStorage::new(dir.path());

        let path = existing_track_storage_path(&storage, "Artist/Album/01.flac")
            .await
            .unwrap();

        assert_eq!(path.as_str(), "Artist/Album/01.flac");
    }

    #[tokio::test]
    async fn existing_track_storage_path_errors_for_missing_local_file() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path());

        let err = existing_track_storage_path(&storage, "Artist/Album/01.flac")
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "INTEGRATION_TRACK_FILE_NOT_FOUND:Artist/Album/01.flac"
        );
    }

    #[tokio::test]
    async fn existing_track_storage_path_errors_for_directory() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(dir.path().join("Artist/Album"))
            .await
            .unwrap();
        let storage = LocalStorage::new(dir.path());

        let err = existing_track_storage_path(&storage, "Artist/Album")
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "INTEGRATION_TRACK_FILE_NOT_FOUND:Artist/Album"
        );
    }

    fn write_minimal_wav(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..64 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[tokio::test]
    async fn apply_release_writes_track_tags_through_storage() {
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(LocalStorage::new(dir.path()));
        let track_rel = "Apply Artist/Apply Album/01 - Old.wav";
        write_minimal_wav(&dir.path().join(track_rel));
        tags::write_tags(
            &dir.path().join(track_rel),
            &TrackTags {
                title: "Old".into(),
                artist: "Old Artist".into(),
                album: "Old Album".into(),
                track_number: Some(1),
                year: None,
                disc_number: None,
                track_total: None,
                disc_total: None,
                genre: None,
                duration_sec: None,
                qobuz_track_id: None,
                qobuz_album_id: None,
                label: None,
                isrc: None,
                composer: None,
            },
        )
        .unwrap();
        let album_id = albums::upsert(
            &pool,
            albums::AlbumUpsert {
                artist_id: None,
                title: "Apply Album",
                year: None,
                qobuz_album_id: None,
                path: Some("Apply Artist/Apply Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let track_id = tracks::upsert(
            &pool,
            tracks::TrackUpsert {
                album_id,
                title: "Old",
                track_number: Some(1),
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: track_rel,
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();
        let release = AlbumMetadataRelease {
            title: "Apply Album Remastered".into(),
            artist_name: "Apply Artist".into(),
            year: Some(2024),
            genre: Some("Post Rock".into()),
            tracks: vec![AlbumMetadataTrack {
                title: "New Title".into(),
                track_number: Some(1),
                disc_number: Some(1),
                year: Some(2024),
                genre: None,
            }],
            cover_url: None,
        };

        let result = apply_release_to_album(
            &ApplyStorageDeps {
                storage: storage.clone(),
            },
            &pool,
            &reqwest::Client::new(),
            album_id,
            &release,
        )
        .await
        .unwrap();

        assert_eq!(result.tracks_updated, 1);
        assert!(result.warnings.is_empty());
        let updated =
            tags::read_tags_storage(storage.as_ref(), &StoragePath::parse(track_rel).unwrap())
                .await
                .unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.album, "Apply Album Remastered");
        assert_eq!(updated.genre.as_deref(), Some("Post Rock"));
        let row = tracks::get_by_id(&pool, track_id).await.unwrap().unwrap();
        assert_eq!(row.title, "New Title");
        assert_eq!(row.year, Some(2024));
    }
}

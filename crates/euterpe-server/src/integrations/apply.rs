use std::path::Path;

use reqwest::Client;
use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::db::{albums, artists, tracks};
use crate::error::ApiError;
use crate::integrations::types::{AlbumLookupContext, AlbumLookupTrack, AlbumMetadataRelease};
use crate::library::covers::{self, detect_cover_mime_type};
use crate::library::paths::library_path_hints;
use crate::library::tags::{self, apply_patch, TrackTagsPatch};

fn path_hints_for_album(
    album_path: Option<&str>,
    track_paths: &[String],
) -> Option<crate::library::paths::LibraryPathHints> {
    if let Some(ap) = album_path.filter(|s| !s.trim().is_empty()) {
        if let Some(first) = track_paths.first() {
            if let Some(name) = Path::new(first).file_name().and_then(|n| n.to_str()) {
                let synthetic = format!("{ap}/{name}");
                if let Some(h) = library_path_hints(&synthetic) {
                    return Some(h);
                }
            }
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
        (
            h.artist_name,
            h.album_title,
            h.year.or(album.year),
        )
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

pub async fn apply_release_to_album(
    config: &AppConfig,
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
    let album_dir = config.library_path.join(album_rel);
    if !album_dir.is_dir() {
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
        let file_path = config.library_path.join(&db_track.path);
        if !file_path.is_file() {
            warnings.push(format!("file missing: {}", db_track.path));
            continue;
        }
        let current = tags::read_tags(&file_path)?;
        let patch = TrackTagsPatch {
            title: Some(meta.title.clone()),
            artist: Some(release.artist_name.clone()),
            album: Some(release.title.clone()),
            track_number: meta.track_number,
            year: meta
                .year
                .or(release.year.map(|y| y as u32)),
            disc_number: meta.disc_number,
            genre: meta.genre.clone().or_else(|| release.genre.clone()),
        };
        let updated = apply_patch(&current, &patch);
        tags::write_tags(&file_path, &updated)?;
        tracks::update_metadata(
            pool,
            db_track.id,
            tracks::TrackMetadataUpdate {
                title: &meta.title,
                track_number: meta.track_number.map(|n| n as i32),
                year: meta.year.map(|y| y as i32).or(release.year),
                disc_number: meta.disc_number.map(|d| d as i32),
                genre: meta
                    .genre
                    .as_deref()
                    .or(release.genre.as_deref()),
                file_mtime: None,
            },
        )
        .await?;
        tracks_updated += 1;
    }

    let mut cover_applied = false;
    if let Some(ref url) = release.cover_url {
        match apply_cover(http, pool, config, &album_dir, album_rel, album_id, url).await {
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
        if let Ok(idx) = usize::try_from(n) {
            if (1..=meta_tracks.len()).contains(&idx) {
                return Some(&meta_tracks[idx - 1]);
            }
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
    config: &AppConfig,
    album_dir: &Path,
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
    let mime = detect_cover_mime_type(content_type.as_deref(), &bytes);
    let ext = match mime {
        lofty::picture::MimeType::Png => "png",
        lofty::picture::MimeType::Gif => "gif",
        _ => "jpg",
    };
    let mut entries = tokio::fs::read_dir(&album_dir)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "folder.jpg" || (name.starts_with("cover.") && name.len() > "cover.".len()) {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
    let rel_cover = format!("{album_rel}/cover.{ext}");
    let cover_path = album_dir.join(format!("cover.{ext}"));
    tokio::fs::write(&cover_path, &bytes)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    albums::set_cover_path(pool, album_id, &rel_cover).await?;
    let track_rows = tracks::list_by_album(pool, album_id).await?;
    for t in track_rows {
        let fp = config.library_path.join(&t.path);
        if fp.is_file() {
            let _ = covers::embed_cover_in_track(&fp, &bytes, &mime);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{match_track, path_hints_for_album};
    use crate::integrations::types::AlbumMetadataTrack;

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
}

use std::path::{Path, PathBuf};

use euterpe_qobuz::Image;
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::MimeType;
use lofty::read_from_path;
use reqwest::Client;
use reqwest::header::CONTENT_TYPE;

use crate::db::albums;
use crate::error::ApiError;
use crate::library::paths::track_path;
use crate::library::tags;
use euterpe_qobuz::{AlbumDetail, Quality};

/// Strip `; charset=...` and trim from `Content-Type`.
fn primary_mime_from_header(raw: &str) -> String {
    raw.split(';')
        .next()
        .unwrap_or(raw)
        .trim()
        .to_ascii_lowercase()
}

/// Prefer HTTP `Content-Type`, then magic bytes; default JPEG if unknown.
pub fn detect_cover_mime_type(content_type: Option<&str>, bytes: &[u8]) -> MimeType {
    if let Some(raw) = content_type.map(str::trim).filter(|s| !s.is_empty()) {
        let primary = primary_mime_from_header(raw);
        let m = MimeType::from_str(&primary);
        if !matches!(m, MimeType::Unknown(_)) {
            return m;
        }
        if primary == "image/webp" {
            return MimeType::Unknown("image/webp".into());
        }
    }

    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return MimeType::Jpeg;
    }
    if bytes.len() >= 8 && bytes[0..8] == *b"\x89PNG\r\n\x1a\n" {
        return MimeType::Png;
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return MimeType::Unknown("image/webp".into());
    }
    if bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        return MimeType::Gif;
    }

    MimeType::Jpeg
}

fn cover_file_extension(mime: &MimeType) -> &'static str {
    match mime {
        MimeType::Jpeg => "jpg",
        MimeType::Png => "png",
        MimeType::Gif => "gif",
        MimeType::Tiff => "tif",
        MimeType::Bmp => "bmp",
        MimeType::Unknown(s) if s.eq_ignore_ascii_case("image/webp") => "webp",
        MimeType::Unknown(_) => "jpg",
        _ => "jpg",
    }
}

/// Remove legacy `folder.jpg` and any `cover.*` so a single canonical `cover.<ext>` remains.
async fn remove_previous_album_cover_files(album_dir: &Path) -> Result<(), ApiError> {
    let legacy = album_dir.join("folder.jpg");
    let _ = tokio::fs::remove_file(&legacy).await;

    let mut read_dir = tokio::fs::read_dir(album_dir)
        .await
        .map_err(|e| ApiError::Message(format!("read album dir: {e}")))?;
    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| ApiError::Message(format!("read dir entry: {e}")))?
    {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "folder.jpg" || (name.starts_with("cover.") && name.len() > "cover.".len()) {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
    Ok(())
}

/// Qobuz catalog ids to try when attaching `cover_path` to `albums` after download.
/// Prefer `download_jobs.qobuz_id` first so it matches `qobuz_favorites` / `register_download`.
fn qobuz_catalog_ids_for_cover(
    download_job_catalog_id: Option<u64>,
    album: &AlbumDetail,
) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::new();
    let mut push = |id: u64| {
        if !out.contains(&id) {
            out.push(id);
        }
    };
    if let Some(id) = download_job_catalog_id {
        push(id);
    }
    if let Some(id) = album.summary.qobuz_id {
        push(id);
    }
    push(album.summary.id);
    out
}

fn is_album_cover_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "folder.jpg" {
        return true;
    }
    if let Some(ext) = lower.strip_prefix("cover.") {
        return matches!(ext, "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp");
    }
    false
}

/// Find `cover.<ext>` or legacy `folder.jpg` under an album directory (relative to library root).
pub fn discover_album_cover_rel(library_root: &Path, album_rel_dir: &str) -> Option<String> {
    let album_rel = album_rel_dir.trim().trim_end_matches('/').replace('\\', "/");
    if album_rel.is_empty() {
        return None;
    }
    let album_dir = library_root.join(&album_rel);
    if !album_dir.is_dir() {
        return None;
    }
    const PREFERRED: &[&str] = &[
        "cover.jpg",
        "cover.jpeg",
        "cover.png",
        "cover.webp",
        "cover.gif",
        "folder.jpg",
    ];
    for name in PREFERRED {
        if album_dir.join(name).is_file() {
            return Some(format!("{album_rel}/{name}"));
        }
    }
    let mut names: Vec<String> = std::fs::read_dir(&album_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_album_cover_filename(n))
        .collect();
    names.sort();
    names
        .first()
        .map(|name| format!("{album_rel}/{name}"))
}

/// If `cover_path` is missing or stale, discover a file on disk and persist it on the album row.
pub async fn ensure_album_cover_path(
    pool: &sqlx::SqlitePool,
    library_root: &Path,
    album_id: i64,
    album_path: Option<&str>,
    current_cover: Option<&str>,
) -> Result<Option<String>, ApiError> {
    if let Some(rel) = current_cover.map(str::trim).filter(|s| !s.is_empty()) {
        if resolve_library_relative_file(library_root, rel).is_ok() {
            return Ok(Some(rel.to_string()));
        }
    }
    let Some(dir) = album_path.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(current_cover.map(str::to_string));
    };
    let Some(rel) = discover_album_cover_rel(library_root, dir) else {
        return Ok(None);
    };
    albums::set_cover_path(pool, album_id, &rel).await?;
    Ok(Some(rel))
}

/// Resolve a library-relative path (POSIX `rel` under `library_root`). Rejects `..` and path
/// components that are not plain single-segment names (no absolute paths, no backslashes).
pub fn resolve_library_relative_file(library_root: &Path, rel: &str) -> Result<PathBuf, ApiError> {
    let rel = rel.trim().trim_start_matches(['/', '\\']);
    if rel.is_empty() {
        return Err(ApiError::Message("cover file not found".into()));
    }
    if rel.contains('\\') || rel.split('/').any(|c| c == ".." || c.is_empty()) {
        return Err(ApiError::bad_request("invalid library path"));
    }
    let root = library_root
        .canonicalize()
        .map_err(|e| ApiError::Message(format!("library root: {e}")))?;
    let candidate = root.join(rel);
    if !candidate.is_file() {
        return Err(ApiError::Message("cover file not found".into()));
    }
    let abs = candidate
        .canonicalize()
        .map_err(|_| ApiError::Message("cover file not found".into()))?;
    if !abs.starts_with(&root) {
        return Err(ApiError::bad_request("invalid library path"));
    }
    Ok(abs)
}

pub fn image_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

pub fn cover_url(image: &Image) -> Option<&str> {
    image
        .large
        .as_deref()
        .or(image.thumbnail.as_deref())
        .or(image.small.as_deref())
        .filter(|u| !u.is_empty())
}

pub async fn download_album_cover(
    http: &Client,
    album_dir: &Path,
    image: &Image,
) -> Result<(PathBuf, MimeType), ApiError> {
    let url = cover_url(image).ok_or_else(|| ApiError::Message("no cover url".into()))?;
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?
        .error_for_status()
        .map_err(|e| ApiError::Message(e.to_string()))?;
    let content_type: Option<String> = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    let mime = detect_cover_mime_type(content_type.as_deref(), &bytes);
    let ext = cover_file_extension(&mime);

    remove_previous_album_cover_files(album_dir).await?;

    tokio::fs::create_dir_all(album_dir)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    let cover_path = album_dir.join(format!("cover.{ext}"));
    tokio::fs::write(&cover_path, &bytes)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    Ok((cover_path, mime))
}

pub fn embed_cover_in_track(
    track_file: &Path,
    cover_bytes: &[u8],
    mime: &MimeType,
) -> Result<(), ApiError> {
    if !tags::is_audio_file(track_file) {
        return Ok(());
    }
    let mut tagged = read_from_path(track_file)
        .map_err(|e| ApiError::Message(format!("read {}: {e}", track_file.display())))?;
    let tag_type = tagged.primary_tag_type();
    let mut tag = tagged
        .primary_tag()
        .cloned()
        .unwrap_or_else(|| lofty::tag::Tag::new(tag_type));
    let picture = lofty::picture::Picture::new_unchecked(
        lofty::picture::PictureType::CoverFront,
        Some(mime.clone()),
        None,
        cover_bytes.to_vec(),
    );
    tag.set_picture(0, picture);
    tagged.insert_tag(tag);
    tagged
        .save_to_path(track_file, WriteOptions::default())
        .map_err(|e| ApiError::Message(format!("embed cover {}: {e}", track_file.display())))?;
    Ok(())
}

pub async fn apply_album_cover_after_download(
    http: &Client,
    pool: &sqlx::SqlitePool,
    library_root: &Path,
    album: &AlbumDetail,
    quality: Quality,
    download_job_catalog_id: Option<u64>,
) -> Result<(), ApiError> {
    let Some(ref image) = album.summary.image else {
        return Ok(());
    };
    let Some(first_track) = album.tracks.as_ref().and_then(|t| t.items.first()) else {
        return Ok(());
    };
    let track_file = track_path(library_root, album, first_track, quality.format_id());
    let album_dir = track_file
        .parent()
        .ok_or_else(|| ApiError::Message("track has no parent dir".into()))?;
    let (cover_path, mime) = download_album_cover(http, album_dir, image).await?;
    let rel_cover = cover_path
        .strip_prefix(library_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| cover_path.to_string_lossy().into_owned());

    for qid in qobuz_catalog_ids_for_cover(download_job_catalog_id, album) {
        if let Some(album_id) = albums::find_id_by_qobuz_album_id(pool, qid as i64).await? {
            albums::set_cover_path(pool, album_id, &rel_cover).await?;
            break;
        }
    }

    let bytes = tokio::fs::read(&cover_path)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    if let Some(tracks) = album.tracks.as_ref() {
        for track in &tracks.items {
            let path = track_path(library_root, album, track, quality.format_id());
            if path.is_file() {
                let _ = embed_cover_in_track(&path, &bytes, &mime);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod mime_tests {
    use super::*;

    #[test]
    fn detect_prefers_content_type_over_wrong_magic() {
        let png_header = b"\x89PNG\r\n\x1a\n";
        let mime = detect_cover_mime_type(Some("image/jpeg"), png_header.as_slice());
        assert!(matches!(mime, MimeType::Jpeg));
    }

    #[test]
    fn detect_png_from_magic_when_no_header() {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.extend_from_slice(&[0u8; 20]);
        let mime = detect_cover_mime_type(None, &b);
        assert!(matches!(mime, MimeType::Png));
        assert_eq!(cover_file_extension(&mime), "png");
    }

    #[test]
    fn detect_webp_from_magic() {
        let mut b = b"RIFF".to_vec();
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(b"WEBP");
        b.extend_from_slice(&[0u8; 8]);
        let mime = detect_cover_mime_type(None, &b);
        assert!(matches!(mime, MimeType::Unknown(_)));
        assert_eq!(cover_file_extension(&mime), "webp");
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn discover_finds_cover_jpg() {
        let dir = tempfile::TempDir::new().unwrap();
        let album = dir.path().join("Artist").join("Album");
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("cover.jpg"), b"img").unwrap();
        std::fs::write(album.join("01 - Song.flac"), b"x").unwrap();
        let rel = discover_album_cover_rel(dir.path(), "Artist/Album").unwrap();
        assert_eq!(rel, "Artist/Album/cover.jpg");
    }

    #[test]
    fn resolve_rejects_double_dot() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(resolve_library_relative_file(dir.path(), "a/../b.jpg").is_err());
    }

    #[test]
    fn resolve_ok_for_file_under_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let rel = "sub/cover.jpg";
        let parent = dir.path().join("sub");
        std::fs::create_dir_all(&parent).unwrap();
        let f = parent.join("cover.jpg");
        std::fs::File::create(&f).unwrap().write_all(b"x").unwrap();
        let got = resolve_library_relative_file(dir.path(), rel).unwrap();
        assert_eq!(got, f.canonicalize().unwrap());
    }
}

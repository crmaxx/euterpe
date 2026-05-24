use std::borrow::Cow;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use euterpe_qobuz::Image;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageReader};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::MimeType;
use lofty::read_from_path;
use reqwest::Client;
use reqwest::header::CONTENT_TYPE;

use crate::db::{albums, tracks};
use crate::error::ApiError;
use crate::library::paths::track_path;
use crate::library::tags;
use euterpe_qobuz::{AlbumDetail, Quality};

/// Maximum uploaded album cover size (20 MiB).
pub const MAX_ALBUM_COVER_BYTES: usize = 20 * 1024 * 1024;

/// Max embedded cover file size (matches qobuz-dl-go `embedMaxSize`).
pub const EMBED_MAX_COVER_BYTES: usize = 2 * 1024 * 1024;
/// Max width/height for embedded cover (matches qobuz-dl-go `embedMaxDim`).
pub const EMBED_MAX_COVER_DIMENSION: u32 = 1600;
const EMBED_MIN_JPEG_QUALITY: u8 = 60;
const EMBED_JPEG_QUALITIES: [u8; 5] = [95, 85, 75, 65, EMBED_MIN_JPEG_QUALITY];

#[derive(Debug, Clone)]
pub struct WriteAlbumCoverResult {
    pub cover_path: String,
    pub tracks_embedded: u32,
}

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
    if bytes.len() >= 2 && bytes[0] == b'B' && bytes[1] == b'M' {
        return MimeType::Bmp;
    }

    MimeType::Jpeg
}

fn validate_upload_content_type_header(content_type: Option<&str>) -> Result<(), ApiError> {
    let Some(raw) = content_type.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let primary = primary_mime_from_header(raw);
    let ok = matches!(
        primary.as_str(),
        "image/jpeg" | "image/jpg" | "image/png" | "image/webp" | "image/bmp"
    );
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad_request("unsupported cover image type"))
    }
}

/// MIME types accepted for `PUT …/albums/{id}/cover` (no GIF).
pub fn is_allowed_upload_cover_mime(mime: &MimeType) -> bool {
    match mime {
        MimeType::Jpeg | MimeType::Png | MimeType::Bmp => true,
        MimeType::Unknown(s) if s.eq_ignore_ascii_case("image/webp") => true,
        _ => false,
    }
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

fn cover_candidate_ext(name: &str) -> Option<&str> {
    let ext = Path::new(name).extension()?.to_str()?;
    let lower = ext.to_ascii_lowercase();
    match lower.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" => Some(ext),
        _ => None,
    }
}

fn normalized_cover_name_tokens(value: &str) -> Vec<String> {
    let stem = Path::new(value)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(value);
    stem.split(|ch: char| !ch.is_alphanumeric())
        .filter_map(|part| {
            let token = part.trim().to_ascii_lowercase();
            if token.is_empty() {
                return None;
            }
            if token.len() == 4 && token.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            Some(token)
        })
        .collect()
}

fn contains_album_title_tokens(file_name: &str, album_title: &str) -> bool {
    let album_tokens = normalized_cover_name_tokens(album_title);
    if album_tokens.is_empty() {
        return false;
    }
    let file_tokens = normalized_cover_name_tokens(file_name);
    file_tokens
        .windows(album_tokens.len())
        .any(|window| window == album_tokens.as_slice())
}

fn rename_album_title_cover_candidate(
    album_dir: &Path,
    album_rel: &str,
    album_title: &str,
) -> Option<String> {
    let mut candidates: Vec<String> = std::fs::read_dir(album_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            cover_candidate_ext(name).is_some() && contains_album_title_tokens(name, album_title)
        })
        .collect();
    candidates.sort();

    let name = candidates.first()?;
    let ext = cover_candidate_ext(name)?;
    let target_name = format!("cover.{ext}");
    let source = album_dir.join(name);
    let target = album_dir.join(&target_name);
    if target.is_file() || std::fs::rename(&source, &target).is_ok() {
        Some(format!("{album_rel}/{target_name}"))
    } else {
        None
    }
}

/// Find `cover.<ext>` or legacy `folder.jpg` under an album directory (relative to library root).
pub fn discover_album_cover_rel(library_root: &Path, album_rel_dir: &str) -> Option<String> {
    let album_rel = album_rel_dir
        .trim()
        .trim_end_matches('/')
        .replace('\\', "/");
    if album_rel.is_empty() {
        return None;
    }
    let album_dir = library_root.join(&album_rel);
    if !album_dir.is_dir() {
        return None;
    }
    for entry in std::fs::read_dir(&album_dir).ok()?.flatten() {
        if !entry.path().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "cover.jpg" | "cover.jpeg" | "cover.png" | "cover.webp" | "cover.bmp" | "folder.jpg"
        ) {
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
    if let Some(name) = names.first() {
        return Some(format!("{album_rel}/{name}"));
    }

    let album_title = album_rel.rsplit('/').next().unwrap_or(&album_rel);
    rename_album_title_cover_candidate(&album_dir, &album_rel, album_title)
}

/// If `cover_path` is missing or stale, discover a file on disk and persist it on the album row.
pub async fn ensure_album_cover_path(
    pool: &sqlx::SqlitePool,
    library_root: &Path,
    album_id: i64,
    album_path: Option<&str>,
    current_cover: Option<&str>,
) -> Result<Option<String>, ApiError> {
    if let Some(rel) = current_cover.map(str::trim).filter(|s| !s.is_empty())
        && resolve_library_relative_file(library_root, rel).is_ok()
    {
        return Ok(Some(rel.to_string()));
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
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Write `cover.<ext>` under the album directory, update DB, embed in all album tracks.
pub async fn write_album_cover_from_bytes(
    pool: &sqlx::SqlitePool,
    library_root: &Path,
    album_id: i64,
    album_rel: &str,
    bytes: &[u8],
    content_type: Option<&str>,
) -> Result<WriteAlbumCoverResult, ApiError> {
    if bytes.is_empty() {
        return Err(ApiError::bad_request("cover image is empty"));
    }
    if bytes.len() > MAX_ALBUM_COVER_BYTES {
        return Err(ApiError::payload_too_large(format!(
            "cover image exceeds {} bytes",
            MAX_ALBUM_COVER_BYTES
        )));
    }
    validate_upload_content_type_header(content_type)?;
    let mime = detect_cover_mime_type(content_type, bytes);
    if !is_allowed_upload_cover_mime(&mime) {
        return Err(ApiError::bad_request("unsupported cover image type"));
    }
    let album_rel = album_rel.trim().replace('\\', "/");
    if album_rel.is_empty() || album_rel.split('/').any(|c| c == ".." || c.is_empty()) {
        return Err(ApiError::bad_request("invalid album path"));
    }
    let album_dir = library_root.join(&album_rel);
    if !album_dir.is_dir() {
        return Err(ApiError::bad_request("album directory not found on disk"));
    }

    remove_previous_album_cover_files(&album_dir).await?;
    let ext = cover_file_extension(&mime);
    let rel_cover = format!("{album_rel}/cover.{ext}");
    let cover_file = album_dir.join(format!("cover.{ext}"));
    tokio::fs::write(&cover_file, bytes)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    albums::set_cover_path(pool, album_id, &rel_cover).await?;

    let mut tracks_embedded = 0u32;
    let track_rows = tracks::list_by_album(pool, album_id).await?;
    for t in track_rows {
        let fp = library_root.join(&t.path);
        if fp.is_file() {
            embed_cover_in_track(&fp, bytes, &mime)?;
            tracks_embedded += 1;
        }
    }

    Ok(WriteAlbumCoverResult {
        cover_path: rel_cover,
        tracks_embedded,
    })
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

/// Optimize cover bytes for ID3/Vorbis picture embedding (resize if >2 MiB and large;
/// re-encode as JPEG with decreasing quality). Mirrors qobuz-dl-go `optimizeCoverForEmbed`.
pub fn optimize_cover_for_embed(data: &[u8]) -> Cow<'_, [u8]> {
    if data.len() <= EMBED_MAX_COVER_BYTES {
        return Cow::Borrowed(data);
    }

    let Ok(img) = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| ())
        .and_then(|r| r.decode().map_err(|_| ()))
    else {
        return Cow::Borrowed(data);
    };

    let mut img = img;
    if img.width() > EMBED_MAX_COVER_DIMENSION || img.height() > EMBED_MAX_COVER_DIMENSION {
        img = resize_cover_image(img, EMBED_MAX_COVER_DIMENSION);
    }

    let mut last = None;
    for quality in EMBED_JPEG_QUALITIES {
        if let Some(bytes) = encode_cover_jpeg(&img, quality) {
            if bytes.len() <= EMBED_MAX_COVER_BYTES {
                return Cow::Owned(bytes);
            }
            last = Some(bytes);
        } else {
            return Cow::Borrowed(data);
        }
    }

    Cow::Owned(last.unwrap_or_else(|| data.to_vec()))
}

fn resize_cover_image(img: DynamicImage, max_dim: u32) -> DynamicImage {
    let (width, height) = img.dimensions();
    let (new_width, new_height) = if width > height {
        (max_dim, height * max_dim / width)
    } else {
        (width * max_dim / height, max_dim)
    };
    img.resize_exact(new_width, new_height, FilterType::CatmullRom)
}

fn encode_cover_jpeg(img: &DynamicImage, quality: u8) -> Option<Vec<u8>> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut buf = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut buf, quality);
    enc.encode(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(buf)
}

pub fn embed_cover_in_track(
    track_file: &Path,
    cover_bytes: &[u8],
    mime: &MimeType,
) -> Result<(), ApiError> {
    if !tags::is_audio_file(track_file) {
        return Ok(());
    }
    let embed_bytes = optimize_cover_for_embed(cover_bytes);
    let embed_mime = match &embed_bytes {
        Cow::Borrowed(_) => mime.clone(),
        Cow::Owned(_) => MimeType::Jpeg,
    };
    let mut tagged = read_from_path(track_file)
        .map_err(|e| ApiError::Message(format!("read {}: {e}", track_file.display())))?;
    let tag_type = tagged.primary_tag_type();
    let mut tag = tagged
        .primary_tag()
        .cloned()
        .unwrap_or_else(|| lofty::tag::Tag::new(tag_type));
    let picture = lofty::picture::Picture::new_unchecked(
        lofty::picture::PictureType::CoverFront,
        Some(embed_mime),
        None,
        embed_bytes.into_owned(),
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
mod embed_tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    #[test]
    fn optimize_leaves_small_png_unchanged() {
        let img = ImageBuffer::from_fn(32, 32, |x, y| Rgb([x as u8, y as u8, 128]));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        assert!(buf.len() <= EMBED_MAX_COVER_BYTES);
        let out = optimize_cover_for_embed(&buf);
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn optimize_shrinks_oversized_jpeg() {
        // High-frequency noise compresses poorly; keeps JPEG >2 MiB at q95.
        let img = ImageBuffer::from_fn(3200, 3200, |x, y| {
            Rgb([
                ((x * 17 + y * 31) % 256) as u8,
                ((x + y * 7) % 256) as u8,
                ((x ^ y) % 256) as u8,
            ])
        });
        let dynamic = DynamicImage::ImageRgb8(img);
        let rgb = dynamic.to_rgb8();
        let (w, h) = rgb.dimensions();
        let mut buf = Vec::new();
        JpegEncoder::new_with_quality(&mut buf, 95)
            .encode(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert!(buf.len() > EMBED_MAX_COVER_BYTES);
        let out = optimize_cover_for_embed(&buf);
        assert!(matches!(out, Cow::Owned(_)));
        assert!(out.len() <= EMBED_MAX_COVER_BYTES);
    }
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
    fn detect_bmp_from_magic() {
        let mut b = b"BM".to_vec();
        b.extend_from_slice(&[0u8; 20]);
        let mime = detect_cover_mime_type(None, &b);
        assert!(matches!(mime, MimeType::Bmp));
        assert!(is_allowed_upload_cover_mime(&mime));
    }

    #[test]
    fn gif_detected_but_not_allowed_for_upload() {
        let b = b"GIF89a";
        let mime = detect_cover_mime_type(None, b);
        assert!(matches!(mime, MimeType::Gif));
        assert!(!is_allowed_upload_cover_mime(&mime));
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
    fn discover_renames_album_title_image_to_cover() {
        let dir = tempfile::TempDir::new().unwrap();
        let album = dir.path().join("Artist").join("Album");
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("Album.jpg"), b"img").unwrap();

        let rel = discover_album_cover_rel(dir.path(), "Artist/Album").unwrap();

        assert_eq!(rel, "Artist/Album/cover.jpg");
        assert!(album.join("cover.jpg").is_file());
        assert!(!album.join("Album.jpg").exists());
    }

    #[test]
    fn discover_renames_album_title_image_with_artist_and_year_to_cover() {
        let dir = tempfile::TempDir::new().unwrap();
        let album = dir.path().join("Scorpions").join("Lonesome Crow");
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("Scorpions - Lonesome Crow - 1972.png"), b"img").unwrap();

        let rel = discover_album_cover_rel(dir.path(), "Scorpions/Lonesome Crow").unwrap();

        assert_eq!(rel, "Scorpions/Lonesome Crow/cover.png");
        assert!(album.join("cover.png").is_file());
        assert!(!album.join("Scorpions - Lonesome Crow - 1972.png").exists());
    }

    #[test]
    fn discover_does_not_rename_unrelated_image() {
        let dir = tempfile::TempDir::new().unwrap();
        let album = dir.path().join("Artist").join("Album");
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("booklet.jpg"), b"img").unwrap();

        let rel = discover_album_cover_rel(dir.path(), "Artist/Album");

        assert!(rel.is_none());
        assert!(album.join("booklet.jpg").is_file());
    }

    #[test]
    fn discover_prefers_existing_cover_over_album_title_image() {
        let dir = tempfile::TempDir::new().unwrap();
        let album = dir.path().join("Artist").join("Album");
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("cover.jpg"), b"cover").unwrap();
        std::fs::write(album.join("Album.png"), b"title").unwrap();

        let rel = discover_album_cover_rel(dir.path(), "Artist/Album").unwrap();

        assert_eq!(rel, "Artist/Album/cover.jpg");
        assert!(album.join("Album.png").is_file());
    }

    #[test]
    fn resolve_rejects_double_dot() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(resolve_library_relative_file(dir.path(), "a/../b.jpg").is_err());
    }

    #[tokio::test]
    async fn write_album_cover_from_bytes_updates_db_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let pool = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&pool).await.unwrap();
        let album_path = dir.path().join("Artist").join("Album");
        std::fs::create_dir_all(&album_path).unwrap();
        let artist_id = crate::db::artists::upsert_by_name(&pool, "Artist", None)
            .await
            .unwrap();
        let album_id = albums::upsert(
            &pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album",
                year: None,
                qobuz_album_id: None,
                path: Some("Artist/Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap();

        let png = b"\x89PNG\r\n\x1a\n".to_vec();
        let result = write_album_cover_from_bytes(
            &pool,
            dir.path(),
            album_id,
            "Artist/Album",
            &png,
            Some("image/png"),
        )
        .await
        .unwrap();
        assert_eq!(result.cover_path, "Artist/Album/cover.png");
        assert!(album_path.join("cover.png").is_file());
        let row = albums::get_by_id(&pool, album_id).await.unwrap().unwrap();
        assert_eq!(row.cover_path.as_deref(), Some("Artist/Album/cover.png"));
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

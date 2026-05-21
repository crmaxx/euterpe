use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag};

use crate::error::ApiError;
use crate::library::paths::{library_path_hints, parse_album_dir_component, parse_track_file_stem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u32>,
    pub year: Option<u32>,
    pub disc_number: Option<u32>,
    pub track_total: Option<u32>,
    pub disc_total: Option<u32>,
    pub genre: Option<String>,
    pub duration_sec: Option<u32>,
    pub qobuz_track_id: Option<u64>,
    pub qobuz_album_id: Option<u64>,
    pub label: Option<String>,
    pub isrc: Option<String>,
    pub composer: Option<String>,
}

/// Album-wide tag fields applied to every track file (per-track title/number unchanged).
#[derive(Debug, Clone, Default)]
pub struct AlbumTagsPatch {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub track_total: Option<u32>,
    pub disc_total: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct TrackTagsPatch {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub year: Option<u32>,
    pub disc_number: Option<u32>,
    pub genre: Option<String>,
}

pub fn audio_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("flac") => "audio/flac",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("aac") => "audio/aac",
        Some("ogg") => "audio/ogg",
        Some("opus") => "audio/opus",
        Some("wav") => "audio/wav",
        Some("aiff") | Some("aif") => "audio/aiff",
        _ => "application/octet-stream",
    }
}

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "flac" | "mp3" | "m4a" | "aac" | "ogg" | "opus" | "wav" | "aiff" | "aif" | "wv"
                    | "ape"
            )
        })
        .unwrap_or(false)
}

/// Path is a phase-1 convertible lossless source (extension only; ALAC vs AAC resolved at convert).
pub fn is_convertible_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(euterpe_converter::is_convertible_extension)
}

/// Folder names that are library layout containers, not performing artists.
const GENERIC_ARTIST_DIRS: &[&str] = &["qobuz", "downloads", "torrent", "torrents", "incoming"];

pub fn read_tags(path: &Path) -> Result<TrackTags, ApiError> {
    read_tags_with_rel(path, None)
}

/// Read tags for indexing; falls back to path/filename when lofty cannot parse the file (e.g. some ALAC `.m4a`).
pub fn read_tags_with_rel(path: &Path, path_rel: Option<&str>) -> Result<TrackTags, ApiError> {
    match read_tags_lofty(path) {
        Ok(tags) => Ok(tags),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "lofty tag read failed; indexing from path/filename hints"
            );
            Ok(tags_from_path(path, path_rel))
        }
    }
}

fn read_tags_lofty(path: &Path) -> Result<TrackTags, ApiError> {
    let tagged = Probe::open(path)
        .map_err(|e| ApiError::Message(format!("probe {}: {e}", path.display())))?
        .guess_file_type()
        .map_err(|e| ApiError::Message(format!("guess type {}: {e}", path.display())))?
        .read()
        .map_err(|e| ApiError::Message(format!("read tags {}: {e}", path.display())))?;

    let tag = tagged.primary_tag().or_else(|| tagged.tags().first());

    let (title, artist, album, track_number, year, disc_number, track_total, disc_total, genre) =
        if let Some(tag) = tag {
            (
                tag.title()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Unknown Title".to_string()),
                tag.artist()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Unknown Artist".to_string()),
                tag.album()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Unknown Album".to_string()),
                tag.track(),
                tag.year(),
                tag.disk(),
                tag.track_total(),
                tag.disk_total(),
                tag.genre().map(|g| g.to_string()),
            )
        } else {
            let fallback = tags_from_path(path, None);
            (
                fallback.title,
                fallback.artist,
                fallback.album,
                fallback.track_number,
                fallback.year,
                fallback.disc_number,
                fallback.track_total,
                fallback.disc_total,
                fallback.genre,
            )
        };

    let duration_sec = tagged.properties().duration().as_secs().try_into().ok();
    let (qobuz_track_id, qobuz_album_id) = parse_qobuz_ids(tag);
    let label = tag_string(tag, &ItemKey::Label);
    let isrc = tag_string(tag, &ItemKey::Isrc);
    let composer = tag_string(tag, &ItemKey::Composer);

    Ok(TrackTags {
        title,
        artist,
        album,
        track_number,
        year,
        disc_number,
        track_total,
        disc_total,
        genre,
        duration_sec,
        qobuz_track_id,
        qobuz_album_id,
        label,
        isrc,
        composer,
    })
}

fn is_generic_artist_dir(name: &str) -> bool {
    GENERIC_ARTIST_DIRS.contains(&name.trim().to_ascii_lowercase().as_str())
}

/// Derive a display artist from `Genesis - …` style album folder names.
fn artist_from_album_folder(album_folder: &str) -> String {
    if let Some((artist, _)) = album_folder.split_once(" - ") {
        let artist = artist.trim();
        if !artist.is_empty() {
            return artist.to_string();
        }
    }
    album_folder.trim().to_string()
}

fn tags_from_path(path: &Path, path_rel: Option<&str>) -> TrackTags {
    if let Some(rel) = path_rel.and_then(library_path_hints) {
        let album_component = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let artist = if is_generic_artist_dir(&rel.artist_name) {
            artist_from_album_folder(album_component)
        } else {
            rel.artist_name
        };
        let title = rel
            .track_title
            .unwrap_or_else(|| path_file_title(path));
        return TrackTags {
            title,
            artist,
            album: rel.album_title,
            track_number: rel.track_number,
            year: rel.year.map(|y| y as u32),
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
        };
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown Title");
    let (track_number, title) = parse_track_file_stem(stem);
    let album_dir = path.parent();
    let album_folder = album_dir
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown Album");
    let (year, album_title) = parse_album_dir_component(album_folder);
    let artist = album_dir
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .filter(|name| !is_generic_artist_dir(name))
        .map(|s| s.to_string())
        .unwrap_or_else(|| artist_from_album_folder(album_folder));

    TrackTags {
        title,
        artist,
        album: album_title,
        track_number,
        year: year.map(|y| y as u32),
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
    }
}

fn path_file_title(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown Title");
    parse_track_file_stem(stem).1
}

fn tag_string(tag: Option<&Tag>, key: &ItemKey) -> Option<String> {
    tag.and_then(|t| t.get_string(key))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn parse_qobuz_ids(tag: Option<&Tag>) -> (Option<u64>, Option<u64>) {
    let mut track_id = None;
    let mut album_id = None;
    let Some(tag) = tag else {
        return (None, None);
    };
    for key in [ItemKey::Comment, ItemKey::Description] {
        let Some(comment) = tag.get_string(&key) else {
            continue;
        };
        for token in comment.split_whitespace() {
            if let Some(id) = token.strip_prefix("QOBUZ_TRACK_ID:") {
                track_id = id.trim().parse().ok();
            }
            if let Some(id) = token.strip_prefix("QOBUZ_ALBUM_ID:") {
                album_id = id.trim().parse().ok();
            }
        }
    }
    (track_id, album_id)
}

pub fn apply_album_patch(tags: &TrackTags, patch: &AlbumTagsPatch) -> TrackTags {
    let mut out = tags.clone();
    if let Some(artist) = &patch.artist {
        out.artist = artist.clone();
    }
    if let Some(album) = &patch.album {
        out.album = album.clone();
    }
    if let Some(year) = patch.year {
        out.year = Some(year);
    }
    if let Some(genre) = &patch.genre {
        out.genre = if genre.is_empty() {
            None
        } else {
            Some(genre.clone())
        };
    }
    if let Some(n) = patch.track_total {
        out.track_total = Some(n);
    }
    if let Some(n) = patch.disc_total {
        out.disc_total = Some(n);
    }
    out
}

pub fn apply_patch(tags: &TrackTags, patch: &TrackTagsPatch) -> TrackTags {
    TrackTags {
        title: patch.title.clone().unwrap_or_else(|| tags.title.clone()),
        artist: patch.artist.clone().unwrap_or_else(|| tags.artist.clone()),
        album: patch.album.clone().unwrap_or_else(|| tags.album.clone()),
        track_number: patch.track_number.or(tags.track_number),
        year: patch.year.or(tags.year),
        disc_number: patch.disc_number.or(tags.disc_number),
        track_total: tags.track_total,
        disc_total: tags.disc_total,
        genre: patch.genre.clone().or_else(|| tags.genre.clone()),
        duration_sec: tags.duration_sec,
        qobuz_track_id: tags.qobuz_track_id,
        qobuz_album_id: tags.qobuz_album_id,
        label: tags.label.clone(),
        isrc: tags.isrc.clone(),
        composer: tags.composer.clone(),
    }
}

pub fn write_tags(path: &Path, tags: &TrackTags) -> Result<(), ApiError> {
    let mut tagged = Probe::open(path)
        .map_err(|e| ApiError::Message(format!("probe {}: {e}", path.display())))?
        .guess_file_type()
        .map_err(|e| ApiError::Message(format!("guess type {}: {e}", path.display())))?
        .read()
        .map_err(|e| ApiError::Message(format!("read {}: {e}", path.display())))?;

    let tag_type = tagged.primary_tag_type();
    let mut tag = tagged
        .primary_tag()
        .cloned()
        .unwrap_or_else(|| Tag::new(tag_type));

    tag.set_title(tags.title.clone());
    tag.set_artist(tags.artist.clone());
    tag.set_album(tags.album.clone());
    if let Some(n) = tags.track_number {
        tag.set_track(n);
    }
    if let Some(y) = tags.year {
        tag.set_year(y);
    }
    if let Some(n) = tags.disc_number {
        tag.set_disk(n);
    }
    if let Some(n) = tags.track_total {
        tag.set_track_total(n);
    }
    if let Some(n) = tags.disc_total {
        tag.set_disk_total(n);
    }
    match &tags.genre {
        Some(g) if !g.is_empty() => {
            tag.set_genre(g.clone());
        }
        Some(_) => {
            tag.remove_genre();
        }
        None => {}
    }
    let mut qobuz_comment = String::new();
    if let Some(tid) = tags.qobuz_track_id {
        qobuz_comment.push_str(&format!("QOBUZ_TRACK_ID:{tid}"));
    }
    if let Some(aid) = tags.qobuz_album_id {
        if !qobuz_comment.is_empty() {
            qobuz_comment.push(' ');
        }
        qobuz_comment.push_str(&format!("QOBUZ_ALBUM_ID:{aid}"));
    }
    if !qobuz_comment.is_empty() {
        tag.insert_text(ItemKey::Comment, qobuz_comment);
    }
    if let Some(label) = &tags.label
        && !label.is_empty()
    {
        tag.insert_text(ItemKey::Label, label.clone());
    }
    if let Some(isrc) = &tags.isrc
        && !isrc.is_empty()
    {
        tag.insert_text(ItemKey::Isrc, isrc.clone());
    }
    if let Some(composer) = &tags.composer
        && !composer.is_empty()
    {
        tag.insert_text(ItemKey::Composer, composer.clone());
    }

    tagged.insert_tag(tag);
    tagged
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| ApiError::Message(format!("write tags {}: {e}", path.display())))?;
    Ok(())
}

pub async fn write_qobuz_tags_async(path: &Path, tags: TrackTags) -> Result<(), ApiError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || write_tags(&path, &tags))
        .await
        .map_err(|e| ApiError::Message(format!("spawn_blocking write tags: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_test_wav(path: &Path, tags: &TrackTags) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..512 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
        write_tags(path, tags).unwrap();
    }

    #[test]
    fn read_write_round_trip_wav() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("01.wav");
        let original = TrackTags {
            title: "Test Song".into(),
            artist: "Test Artist".into(),
            album: "Test Album".into(),
            track_number: Some(3),
            year: Some(2024),
            disc_number: Some(2),
            track_total: Some(12),
            disc_total: Some(2),
            genre: Some("Rock".into()),
            duration_sec: None,
            qobuz_track_id: Some(999),
            qobuz_album_id: Some(4242),
            label: Some("Indie Records".into()),
            isrc: Some("USRC17607839".into()),
            composer: Some("Test Composer".into()),
        };
        write_test_wav(&path, &original);
        let read = read_tags(&path).unwrap();
        assert_eq!(read.title, "Test Song");
        assert_eq!(read.artist, "Test Artist");
        assert_eq!(read.album, "Test Album");
        assert_eq!(read.track_number, Some(3));
        assert_eq!(read.year, Some(2024));
        assert_eq!(read.disc_number, Some(2));
        assert_eq!(read.track_total, Some(12));
        assert_eq!(read.disc_total, Some(2));
        assert_eq!(read.genre.as_deref(), Some("Rock"));
        assert_eq!(read.label.as_deref(), Some("Indie Records"));
        assert_eq!(read.isrc.as_deref(), Some("USRC17607839"));
        assert_eq!(read.composer.as_deref(), Some("Test Composer"));

        let flac_path = dir.path().join("tagged.flac");
        std::fs::write(
            &flac_path,
            include_bytes!("../../tests/fixtures/silent.flac"),
        )
        .unwrap();
        write_tags(&flac_path, &original).unwrap();
        let flac_read = read_tags(&flac_path).unwrap();
        assert_eq!(flac_read.qobuz_track_id, Some(999));
        assert_eq!(flac_read.qobuz_album_id, Some(4242));
    }

    #[test]
    fn apply_patch_changes_fields() {
        let tags = TrackTags {
            title: "A".into(),
            artist: "B".into(),
            album: "C".into(),
            track_number: Some(1),
            year: Some(2000),
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
        };
        let patched = apply_patch(
            &tags,
            &TrackTagsPatch {
                title: Some("New".into()),
                ..Default::default()
            },
        );
        assert_eq!(patched.title, "New");
        assert_eq!(patched.artist, "B");
    }

    #[test]
    fn apply_album_patch_preserves_per_track_fields() {
        let tags = TrackTags {
            title: "Song A".into(),
            artist: "Old Artist".into(),
            album: "Old Album".into(),
            track_number: Some(3),
            year: Some(2000),
            disc_number: Some(1),
            track_total: None,
            disc_total: None,
            genre: None,
            duration_sec: None,
            qobuz_track_id: None,
            qobuz_album_id: None,
            label: None,
            isrc: None,
            composer: None,
        };
        let patched = apply_album_patch(
            &tags,
            &AlbumTagsPatch {
                artist: Some("New Artist".into()),
                album: Some("New Album".into()),
                track_total: Some(10),
                disc_total: Some(2),
                ..Default::default()
            },
        );
        assert_eq!(patched.title, "Song A");
        assert_eq!(patched.track_number, Some(3));
        assert_eq!(patched.disc_number, Some(1));
        assert_eq!(patched.artist, "New Artist");
        assert_eq!(patched.track_total, Some(10));
        assert_eq!(patched.disc_total, Some(2));
    }

    #[test]
    fn is_audio_file_recognizes_extensions() {
        assert!(is_audio_file(Path::new("/a/track.flac")));
        assert!(is_audio_file(Path::new("/a/track.MP3")));
        assert!(is_audio_file(Path::new("/a/track.m4a")));
        assert!(!is_audio_file(Path::new("/a/readme.txt")));
    }

    #[test]
    fn tags_from_path_qobuz_layout_uses_album_folder_artist() {
        let rel = "Qobuz/Genesis - The Lamb Lies Down On Broadway [Cartridge 2] UK 1974/Genesis - The Lamb Lies Down On Broadway [Cartridge 2].m4a";
        let path = Path::new("/music").join(rel);
        let tags = tags_from_path(&path, Some(rel));
        assert_eq!(tags.artist, "Genesis");
        assert!(tags.album.contains("Lamb Lies Down"));
        assert!(tags.title.contains("Lamb") || tags.title.contains("Genesis"));
    }

    #[test]
    fn tags_from_path_standard_artist_album_layout() {
        let rel = "Genesis/1974 - The Lamb/01 - In The Cage.flac";
        let path = Path::new("/music").join(rel);
        let tags = tags_from_path(&path, Some(rel));
        assert_eq!(tags.artist, "Genesis");
        assert_eq!(tags.album, "The Lamb");
        assert_eq!(tags.title, "In The Cage");
        assert_eq!(tags.track_number, Some(1));
    }
}

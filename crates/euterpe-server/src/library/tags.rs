use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag};

use crate::error::ApiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u32>,
    pub year: Option<u32>,
    pub disc_number: Option<u32>,
    pub genre: Option<String>,
    pub duration_sec: Option<u32>,
    pub qobuz_track_id: Option<u64>,
    pub qobuz_album_id: Option<u64>,
    pub label: Option<String>,
    pub isrc: Option<String>,
    pub composer: Option<String>,
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

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "flac" | "mp3" | "m4a" | "aac" | "ogg" | "opus" | "wav" | "aiff" | "aif"
            )
        })
        .unwrap_or(false)
}

pub fn read_tags(path: &Path) -> Result<TrackTags, ApiError> {
    let tagged = Probe::open(path)
        .map_err(|e| ApiError::Message(format!("probe {}: {e}", path.display())))?
        .guess_file_type()
        .map_err(|e| ApiError::Message(format!("guess type {}: {e}", path.display())))?
        .read()
        .map_err(|e| ApiError::Message(format!("read tags {}: {e}", path.display())))?;

    let tag = tagged
        .primary_tag()
        .or_else(|| tagged.tags().first());

    let (title, artist, album, track_number, year, disc_number, genre) =
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
                tag.genre().map(|g| g.to_string()),
            )
        } else {
            (
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown Title")
                    .to_string(),
                "Unknown Artist".to_string(),
                path.parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown Album")
                    .to_string(),
                None,
                None,
                None,
                None,
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
        genre,
        duration_sec,
        qobuz_track_id,
        qobuz_album_id,
        label,
        isrc,
        composer,
    })
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

pub fn apply_patch(tags: &TrackTags, patch: &TrackTagsPatch) -> TrackTags {
    TrackTags {
        title: patch.title.clone().unwrap_or_else(|| tags.title.clone()),
        artist: patch.artist.clone().unwrap_or_else(|| tags.artist.clone()),
        album: patch.album.clone().unwrap_or_else(|| tags.album.clone()),
        track_number: patch.track_number.or(tags.track_number),
        year: patch.year.or(tags.year),
        disc_number: patch.disc_number.or(tags.disc_number),
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
    let mut tag = tagged.primary_tag().cloned().unwrap_or_else(|| Tag::new(tag_type));

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
    if let Some(label) = &tags.label {
        if !label.is_empty() {
            tag.insert_text(ItemKey::Label, label.clone());
        }
    }
    if let Some(isrc) = &tags.isrc {
        if !isrc.is_empty() {
            tag.insert_text(ItemKey::Isrc, isrc.clone());
        }
    }
    if let Some(composer) = &tags.composer {
        if !composer.is_empty() {
            tag.insert_text(ItemKey::Composer, composer.clone());
        }
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
        assert_eq!(read.genre.as_deref(), Some("Rock"));
        assert_eq!(read.label.as_deref(), Some("Indie Records"));
        assert_eq!(read.isrc.as_deref(), Some("USRC17607839"));
        assert_eq!(read.composer.as_deref(), Some("Test Composer"));

        let flac_path = dir.path().join("tagged.flac");
        std::fs::write(&flac_path, include_bytes!("../../tests/fixtures/silent.flac")).unwrap();
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
    fn is_audio_file_recognizes_extensions() {
        assert!(is_audio_file(Path::new("/a/track.flac")));
        assert!(is_audio_file(Path::new("/a/track.MP3")));
        assert!(!is_audio_file(Path::new("/a/readme.txt")));
    }
}

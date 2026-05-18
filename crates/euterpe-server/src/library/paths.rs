use std::path::{Path, PathBuf};

use euterpe_qobuz::{AlbumDetail, TrackSummary};

pub fn sanitize_component(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => out.push('_'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn extension_for_quality(quality: u8) -> &'static str {
    match quality {
        5 => "mp3",
        _ => "flac",
    }
}

/// Year from Qobuz `release_date_original` (typically `YYYY-MM-DD`).
pub fn year_from_release_date(release_date_original: Option<&str>) -> Option<i32> {
    release_date_original
        .and_then(|s| s.get(0..4))
        .and_then(|y| y.parse().ok())
}

/// Album folder name: `{year} - {title}` (or `Unknown - {title}` when year is missing).
pub fn album_dir_name(title: &str, release_date_original: Option<&str>) -> String {
    let title = sanitize_component(title);
    match year_from_release_date(release_date_original) {
        Some(year) => format!("{year} - {title}"),
        None => format!("Unknown - {title}"),
    }
}

/// Hints parsed from library-relative paths (`Artist/2020 - Album/01 - Title.flac`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPathHints {
    pub artist_name: String,
    pub album_title: String,
    pub year: Option<i32>,
    pub track_title: Option<String>,
    pub track_number: Option<u32>,
}

/// Parse album folder name: `2019 - Album` or `Unknown - Album` or plain `Album`.
pub fn parse_album_dir_component(name: &str) -> (Option<i32>, String) {
    let name = name.trim();
    if let Some((prefix, title)) = name.split_once(" - ") {
        let title = title.trim();
        if let Ok(year) = prefix.trim().parse::<i32>() {
            return (Some(year), title.to_string());
        }
        if prefix.eq_ignore_ascii_case("unknown") {
            return (None, title.to_string());
        }
    }
    (None, name.to_string())
}

/// Parse track file stem: `03 - Song` or `Song`.
pub fn parse_track_file_stem(stem: &str) -> (Option<u32>, String) {
    let stem = stem.trim();
    if let Some((prefix, title)) = stem.split_once(" - ") {
        let title = title.trim();
        if let Ok(n) = prefix.trim().parse::<u32>() {
            return (Some(n), title.to_string());
        }
    }
    (None, stem.to_string())
}

/// Parse a path relative to the library root (forward slashes).
pub fn library_path_hints(rel: &str) -> Option<LibraryPathHints> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() {
        return None;
    }
    let path = Path::new(&rel);
    let file_stem = path.file_stem()?.to_str()?;
    let album_dir = path.parent()?;
    let artist_component = album_dir.parent()?.file_name()?.to_str()?;
    let album_component = album_dir.file_name()?.to_str()?;
    let (year, album_title) = parse_album_dir_component(album_component);
    let (track_number, track_title) = parse_track_file_stem(file_stem);
    Some(LibraryPathHints {
        artist_name: artist_component.to_string(),
        album_title,
        year,
        track_title: Some(track_title),
        track_number,
    })
}

pub fn track_path(
    library_root: &Path,
    album: &AlbumDetail,
    track: &TrackSummary,
    quality: u8,
) -> PathBuf {
    let artist = album
        .summary
        .artist
        .as_ref()
        .map(|a| sanitize_component(&a.name))
        .unwrap_or_else(|| "Unknown Artist".into());
    let album_dir = album_dir_name(
        &album.summary.title,
        album.summary.release_date_original.as_deref(),
    );
    let track_num = track.track_number.unwrap_or(0);
    let title = sanitize_component(&track.title);
    let ext = extension_for_quality(quality);
    let filename = format!("{track_num:02} - {title}.{ext}");
    library_root.join(artist).join(album_dir).join(filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use euterpe_qobuz::{AlbumSummary, ArtistRef};

    #[test]
    fn sanitize_replaces_slashes() {
        assert_eq!(sanitize_component("AC/DC"), "AC_DC");
    }

    #[test]
    fn extension_for_hires_plus_is_flac() {
        assert_eq!(extension_for_quality(27), "flac");
    }

    #[test]
    fn album_dir_name_with_year() {
        assert_eq!(
            album_dir_name("Test Album", Some("2019-06-01")),
            "2019 - Test Album"
        );
        assert_eq!(album_dir_name("Test Album", None), "Unknown - Test Album");
    }

    #[test]
    fn library_path_hints_parses_qobuz_layout() {
        let h = library_path_hints("Artist/2020 - Test Album/03 - Song.flac").unwrap();
        assert_eq!(h.artist_name, "Artist");
        assert_eq!(h.album_title, "Test Album");
        assert_eq!(h.year, Some(2020));
        assert_eq!(h.track_number, Some(3));
        assert_eq!(h.track_title.as_deref(), Some("Song"));
    }

    #[test]
    fn library_path_hints_unknown_year() {
        let h = library_path_hints("A/Unknown - Album/01 - T.flac").unwrap();
        assert_eq!(h.year, None);
        assert_eq!(h.album_title, "Album");
    }

    #[test]
    fn parse_album_dir_plain_title() {
        let (y, t) = parse_album_dir_component("My Album");
        assert_eq!(y, None);
        assert_eq!(t, "My Album");
    }

    #[test]
    fn track_path_layout() {
        let album = AlbumDetail {
            summary: AlbumSummary {
                id: 1,
                qobuz_id: None,
                title: "Test Album".into(),
                artist: Some(ArtistRef {
                    id: 2,
                    name: "Artist".into(),
                }),
                artists: None,
                image: None,
                release_date_original: Some("2020-03-15".into()),
                hires: None,
                album_ref: None,
                slug: None,
                list_id: None,
                product_id: None,
                genre: None,
                label: None,
            },
            tracks: None,
            description: None,
        };
        let track = TrackSummary {
            id: 10,
            title: "Song".into(),
            track_number: Some(3),
            duration: None,
            performer: None,
            hires_streamable: None,
            media_number: None,
            genre: None,
            isrc: None,
            composer: None,
        };
        let path = track_path(Path::new("/music"), &album, &track, 6);
        let s = path.to_string_lossy();
        assert!(s.ends_with("03 - Song.flac"));
        assert!(s.contains("Artist/2020 - Test Album/"));
    }
}

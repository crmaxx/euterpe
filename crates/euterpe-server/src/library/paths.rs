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
        };
        let path = track_path(Path::new("/music"), &album, &track, 6);
        let s = path.to_string_lossy();
        assert!(s.ends_with("03 - Song.flac"));
        assert!(s.contains("Artist/2020 - Test Album/"));
    }
}

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
    let album_name = sanitize_component(&album.summary.title);
    let track_num = track.track_number.unwrap_or(0);
    let title = sanitize_component(&track.title);
    let ext = extension_for_quality(quality);
    let filename = format!("{track_num:02} - {title}.{ext}");
    library_root.join(artist).join(album_name).join(filename)
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
                release_date_original: None,
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
        assert!(path.ends_with("03 - Song.flac"));
        assert!(path.to_string_lossy().contains("Artist"));
    }
}

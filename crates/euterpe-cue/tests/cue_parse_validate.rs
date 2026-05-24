use std::path::Path;

use euterpe_cue::{parse_cue, validate_document};

const VALID_CUE: &str = r#"
REM GENRE "Dance"
REM DATE 2007
REM COMMENT "Vinyl rip"
PERFORMER "Scooter"
TITLE "Jumping All Over The World"
FILE "album.flac" FLAC
  TRACK 01 AUDIO
    TITLE "Radio Edit"
    PERFORMER "Scooter"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Extended Mix"
    PERFORMER "Scooter"
    REM GENRE "Club"
    INDEX 00 00:59:00
    INDEX 01 01:00:00
"#;

#[test]
fn parse_cue_extracts_album_track_and_rem_fields() {
    let doc = parse_cue(VALID_CUE, Path::new("Scooter.cue")).unwrap();

    assert_eq!(doc.cue_path, "Scooter.cue");
    assert_eq!(doc.audio_path, "album.flac");
    assert_eq!(doc.album_artist, "Scooter");
    assert_eq!(doc.album_title, "Jumping All Over The World");
    assert_eq!(doc.year, Some(2007));
    assert_eq!(doc.genre.as_deref(), Some("Dance"));
    assert_eq!(doc.comment.as_deref(), Some("Vinyl rip"));
    assert_eq!(doc.tracks.len(), 2);
    assert_eq!(doc.tracks[0].number, 1);
    assert_eq!(doc.tracks[0].title, "Radio Edit");
    assert_eq!(doc.tracks[0].artist.as_deref(), Some("Scooter"));
    assert_eq!(doc.tracks[0].start_index, "00:00:00");
    assert_eq!(doc.tracks[1].genre.as_deref(), Some("Club"));
    assert_eq!(doc.tracks[1].pregap.as_deref(), Some("00:59:00"));
}

#[test]
fn validate_document_requires_album_fields_and_valid_tracks() {
    let mut doc = parse_cue(VALID_CUE, Path::new("Scooter.cue")).unwrap();
    doc.year = None;
    doc.genre = None;
    doc.album_title.clear();
    doc.tracks[0].title.clear();

    let validation = validate_document(&doc);

    assert!(!validation.valid);
    let codes: Vec<&str> = validation.issues.iter().map(|i| i.code.as_str()).collect();
    assert!(codes.contains(&"missing_album_year"));
    assert!(codes.contains(&"missing_album_genre"));
    assert!(codes.contains(&"missing_album_title"));
    assert!(codes.contains(&"missing_track_title"));
}

#[test]
fn parse_cue_reports_invalid_index_frame_with_location() {
    let invalid = VALID_CUE.replace("INDEX 01 01:00:00", "INDEX 01 01:00:82");

    let err = parse_cue(&invalid, Path::new("bad.cue")).unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("frame"), "{msg}");
    assert!(msg.contains("line"), "{msg}");
}

#[test]
fn parse_cue_rejects_multiple_file_commands() {
    let invalid = VALID_CUE.replace(
        "  TRACK 02 AUDIO",
        "FILE \"second.flac\" FLAC\n  TRACK 02 AUDIO",
    );

    let err = parse_cue(&invalid, Path::new("multi.cue")).unwrap_err();

    assert!(err.to_string().contains("multiple FILE"));
}

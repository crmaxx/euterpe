use std::path::{Path, PathBuf};

use euterpe_converter::{ConvertOptions, FilePolicy, FlacEncodeSettings};
use euterpe_cue::{SourceFilePolicy, SplitOptions, parse_cue, split_flac_image, validate_document};
use lofty::file::TaggedFileExt;
use lofty::prelude::Accessor;

fn write_test_wav(path: &Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for sample in 0..(44_100 * 3) {
        writer.write_sample((sample % 1024) as i16).unwrap();
    }
    writer.finalize().unwrap();
}

fn make_flac_image(dir: &Path) -> PathBuf {
    let wav = dir.join("album.wav");
    write_test_wav(&wav);
    let result = euterpe_converter::convert_file(
        &wav,
        ConvertOptions {
            flac_encode: &FlacEncodeSettings::default(),
            file_policy: FilePolicy::SiblingThenDelete,
            on_progress: None,
        },
    )
    .unwrap();
    result.output_path
}

#[test]
fn split_flac_image_writes_tagged_tracks_and_keeps_source_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let image = make_flac_image(temp.path());
    let cue = r#"
REM GENRE "Dance"
REM DATE 2007
PERFORMER "Album Artist"
TITLE "Album Title"
FILE "album.flac" FLAC
  TRACK 01 AUDIO
    TITLE "First"
    PERFORMER "Track Artist 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Second"
    INDEX 01 00:01:00
"#;
    let doc = parse_cue(cue, Path::new("album.cue")).unwrap();
    assert!(validate_document(&doc).valid);

    let out = temp.path().join("split");
    let result = split_flac_image(
        &doc,
        temp.path(),
        &out,
        &SplitOptions {
            source_file_policy: SourceFilePolicy::Keep,
            file_mask: None,
            on_progress: None,
        },
    )
    .unwrap();

    assert_eq!(result.output_paths.len(), 2);
    assert!(image.exists());
    assert!(
        result.output_paths[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("01")
    );
    assert!(
        result.output_paths[1]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("02")
    );

    let first = lofty::read_from_path(&result.output_paths[0]).unwrap();
    let first_tag = first
        .primary_tag()
        .or_else(|| first.tags().first())
        .unwrap();
    assert_eq!(first_tag.title().as_deref(), Some("First"));
    assert_eq!(first_tag.artist().as_deref(), Some("Track Artist 1"));
    assert_eq!(first_tag.album().as_deref(), Some("Album Title"));
    assert_eq!(first_tag.genre().as_deref(), Some("Dance"));

    let second = lofty::read_from_path(&result.output_paths[1]).unwrap();
    let second_tag = second
        .primary_tag()
        .or_else(|| second.tags().first())
        .unwrap();
    assert_eq!(second_tag.artist().as_deref(), Some("Album Artist"));
}

#[test]
fn split_flac_image_can_delete_source_after_success() {
    let temp = tempfile::tempdir().unwrap();
    let image = make_flac_image(temp.path());
    let cue = r#"
REM GENRE "Dance"
REM DATE 2007
PERFORMER "Artist"
TITLE "Album"
FILE "album.flac" FLAC
  TRACK 01 AUDIO
    TITLE "Only"
    INDEX 01 00:00:00
"#;
    let doc = parse_cue(cue, Path::new("album.cue")).unwrap();

    split_flac_image(
        &doc,
        temp.path(),
        &temp.path().join("split"),
        &SplitOptions {
            source_file_policy: SourceFilePolicy::DeleteAfterSuccess,
            file_mask: None,
            on_progress: None,
        },
    )
    .unwrap();

    assert!(!image.exists());
}

#[test]
fn split_flac_image_encodes_tracks_without_temporary_wav_slices() {
    let temp = tempfile::tempdir().unwrap();
    make_flac_image(temp.path());
    let cue = r#"
REM GENRE "Dance"
REM DATE 2007
PERFORMER "Album Artist"
TITLE "Album Title"
FILE "album.flac" FLAC
  TRACK 01 AUDIO
    TITLE "First"
    PERFORMER "Track Artist 1"
    INDEX 01 00:00:00
"#;
    let doc = parse_cue(cue, Path::new("album.cue")).unwrap();
    let out = temp.path().join("split");
    std::fs::create_dir_all(out.join("01 - Track Artist 1 - First.wav")).unwrap();

    let result = split_flac_image(
        &doc,
        temp.path(),
        &out,
        &SplitOptions {
            source_file_policy: SourceFilePolicy::Keep,
            file_mask: None,
            on_progress: None,
        },
    )
    .unwrap();

    assert_eq!(result.output_paths.len(), 1);
    assert!(out.join("01 - Track Artist 1 - First.flac").exists());
}

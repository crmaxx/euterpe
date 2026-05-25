//! CUE sheet parsing, validation, and FLAC image splitting.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use cue_lib::core::CueTimestamp;
use cue_lib::error::{CueLibError, CueLibErrorKind};
use cue_lib::parse::CuesheetParser;
use euterpe_converter::{FlacEncodeSettings, encode_interleaved_pcm_to_flac_bytes};

pub type Result<T> = std::result::Result<T, CueError>;

#[derive(Debug, thiserror::Error)]
pub enum CueError {
    #[error("cue parse error at line {line}, column {column}: {message}")]
    Parse {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("multiple FILE commands are not supported for one-file CUE splitting")]
    MultipleFiles,
    #[error("invalid CUE: {0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("FLAC decode error: {0}")]
    FlacDecode(#[from] claxon::Error),
    #[error("convert error: {0}")]
    Convert(#[from] euterpe_converter::ConvertError),
    #[error("tag error: {0}")]
    Tags(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueDocument {
    pub cue_path: String,
    pub audio_path: String,
    pub album_title: String,
    pub album_artist: String,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub comment: Option<String>,
    pub extra_fields: Vec<CueExtraField>,
    pub tracks: Vec<CueTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueExtraField {
    pub scope: CueFieldScope,
    pub track_number: Option<u32>,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CueFieldScope {
    Album,
    Track,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueTrack {
    pub number: u32,
    pub artist: Option<String>,
    pub title: String,
    pub genre: Option<String>,
    pub start_index: String,
    pub pregap: Option<String>,
    pub duration: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueIssue {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
    pub track_number: Option<u32>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueValidation {
    pub valid: bool,
    pub issues: Vec<CueIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFilePolicy {
    Keep,
    DeleteAfterSuccess,
}

#[derive(Clone)]
pub struct SplitOptions {
    pub source_file_policy: SourceFilePolicy,
    pub file_mask: Option<String>,
    pub on_progress: Option<std::sync::Arc<dyn Fn(SplitProgress) + Send + Sync>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitProgress {
    pub tracks_done: usize,
    pub tracks_total: usize,
    pub track_number: u32,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitResult {
    pub output_paths: Vec<PathBuf>,
}

pub trait SplitIo {
    fn read_source(&mut self, audio_path: &Path) -> Result<Vec<u8>>;
    fn write_output(&mut self, rel_path: &Path, bytes: Vec<u8>) -> Result<()>;
    fn delete_source(&mut self, rel_path: &Path) -> Result<()>;
}

#[derive(Debug, Default)]
struct CueRemFields {
    year: Option<u32>,
    genre: Option<String>,
    comment: Option<String>,
    album_extra: Vec<(String, String)>,
    track_extra: Vec<(u32, String, String)>,
}

pub fn parse_cue(cue_text: &str, cue_path: &Path) -> Result<CueDocument> {
    if count_file_commands(cue_text) > 1 {
        return Err(CueError::MultipleFiles);
    }

    let cuesheet = CuesheetParser::new()
        .allow_vorbis_remarks(true)
        .parse(cue_text)
        .map_err(map_cue_lib_error)?;

    let audio_path = cuesheet
        .file
        .map(|f| f.name.to_string())
        .ok_or_else(|| CueError::Invalid("missing FILE command".into()))?;

    let rem_fields = parse_rem_fields(cue_text);
    let mut tracks = Vec::new();
    for track in cuesheet.tracks {
        let number = track.no.to_string().parse::<u32>().unwrap_or(0);
        let rem = rem_fields
            .track_extra
            .iter()
            .filter(|(n, _, _)| *n == number)
            .collect::<Vec<_>>();
        let track_genre = rem
            .iter()
            .find(|(_, k, _)| k.eq_ignore_ascii_case("GENRE"))
            .map(|(_, _, v)| v.clone());
        tracks.push(CueTrack {
            number,
            artist: track.performer.map(|v| v.to_string()),
            title: track.title.map(|v| v.to_string()).unwrap_or_default(),
            genre: track_genre,
            start_index: track.time_info.start.to_string(),
            pregap: track.time_info.pregap_start.map(|v| v.to_string()),
            duration: track.time_info.duration.map(|v| v.to_string()),
            selected: true,
        });
    }

    let mut extra_fields = rem_fields
        .album_extra
        .into_iter()
        .map(|(key, value)| CueExtraField {
            scope: CueFieldScope::Album,
            track_number: None,
            key,
            value,
        })
        .collect::<Vec<_>>();
    extra_fields.extend(
        rem_fields
            .track_extra
            .into_iter()
            .filter(|(_, key, _)| !key.eq_ignore_ascii_case("GENRE"))
            .map(|(track_number, key, value)| CueExtraField {
                scope: CueFieldScope::Track,
                track_number: Some(track_number),
                key,
                value,
            }),
    );

    Ok(CueDocument {
        cue_path: cue_path.to_string_lossy().replace('\\', "/"),
        audio_path,
        album_title: cuesheet
            .album_title
            .map(|v| v.to_string())
            .unwrap_or_default(),
        album_artist: cuesheet
            .performer
            .map(|v| v.to_string())
            .unwrap_or_default(),
        year: rem_fields.year,
        genre: rem_fields.genre,
        comment: rem_fields.comment,
        extra_fields,
        tracks,
    })
}

pub fn validate_document(document: &CueDocument) -> CueValidation {
    let mut issues = Vec::new();
    require_text(
        &mut issues,
        "album_title",
        &document.album_title,
        "missing_album_title",
    );
    require_text(
        &mut issues,
        "album_artist",
        &document.album_artist,
        "missing_album_artist",
    );
    if document.year.is_none() {
        issues.push(issue(
            "missing_album_year",
            "Album year is required",
            Some("year"),
            None,
        ));
    }
    if document.genre.as_deref().unwrap_or("").trim().is_empty() {
        issues.push(issue(
            "missing_album_genre",
            "Album genre is required",
            Some("genre"),
            None,
        ));
    }
    if document.tracks.is_empty() {
        issues.push(issue(
            "missing_tracks",
            "At least one track is required",
            None,
            None,
        ));
    }
    for track in &document.tracks {
        if track.title.trim().is_empty() {
            issues.push(issue(
                "missing_track_title",
                "Track title is required",
                Some("tracks.title"),
                Some(track.number),
            ));
        }
        if parse_timestamp_frames(&track.start_index).is_none() {
            issues.push(issue(
                "invalid_track_index",
                "Track INDEX must be mm:ss:ff with frame 00..74",
                Some("tracks.start_index"),
                Some(track.number),
            ));
        }
    }
    CueValidation {
        valid: issues.is_empty(),
        issues,
    }
}

pub fn split_flac_image(
    document: &CueDocument,
    cue_dir: &Path,
    output_dir: &Path,
    options: &SplitOptions,
) -> Result<SplitResult> {
    let mut io = LocalSplitIo {
        cue_dir: cue_dir.to_path_buf(),
    };
    split_flac_image_io(document, &mut io, output_dir, options)
}

struct LocalSplitIo {
    cue_dir: PathBuf,
}

impl SplitIo for LocalSplitIo {
    fn read_source(&mut self, audio_path: &Path) -> Result<Vec<u8>> {
        fs::read(self.cue_dir.join(audio_path)).map_err(CueError::Io)
    }

    fn write_output(&mut self, rel_path: &Path, bytes: Vec<u8>) -> Result<()> {
        if let Some(parent) = rel_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(rel_path, bytes).map_err(CueError::Io)
    }

    fn delete_source(&mut self, rel_path: &Path) -> Result<()> {
        fs::remove_file(self.cue_dir.join(rel_path)).map_err(CueError::Io)
    }
}

pub fn split_flac_image_io(
    document: &CueDocument,
    io: &mut impl SplitIo,
    output_dir_rel: &Path,
    options: &SplitOptions,
) -> Result<SplitResult> {
    let validation = validate_document(document);
    if !validation.valid {
        return Err(CueError::Invalid("document has validation errors".into()));
    }
    if !document.audio_path.to_ascii_lowercase().ends_with(".flac") {
        return Err(CueError::Invalid("CUE split input must be FLAC".into()));
    }

    let source_rel = Path::new(&document.audio_path);
    let source_bytes = io.read_source(source_rel)?;
    let mut reader = claxon::FlacReader::new(Cursor::new(source_bytes))?;
    let info = reader.streaminfo();
    let channels = info.channels as usize;
    let sample_rate = info.sample_rate;
    let bits_per_sample = info.bits_per_sample;
    let samples = reader
        .samples()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let frames_total = samples.len() / channels;

    let mut output_paths = Vec::new();
    let selected = document
        .tracks
        .iter()
        .filter(|t| t.selected)
        .collect::<Vec<_>>();
    for (idx, track) in selected.iter().enumerate() {
        let start = timestamp_to_sample(&track.start_index, sample_rate)
            .ok_or_else(|| CueError::Invalid("invalid track index".into()))?
            .min(frames_total);
        let end = selected
            .get(idx + 1)
            .and_then(|next| timestamp_to_sample(&next.start_index, sample_rate))
            .unwrap_or(frames_total)
            .min(frames_total)
            .max(start);
        let stem = output_stem(document, track, options.file_mask.as_deref());
        let output_path = output_dir_rel.join(format!("{stem}.flac"));
        let encoded = encode_interleaved_pcm_to_flac_bytes(
            &samples[start * channels..end * channels],
            channels,
            bits_per_sample as usize,
            sample_rate as usize,
            &FlacEncodeSettings::default(),
            None,
        )?;
        let tagged = write_track_tags_to_flac_bytes(encoded, document, track)?;
        io.write_output(&output_path, tagged)?;
        output_paths.push(output_path.clone());
        if let Some(on_progress) = &options.on_progress {
            on_progress(SplitProgress {
                tracks_done: output_paths.len(),
                tracks_total: selected.len(),
                track_number: track.number,
                output_path,
            });
        }
    }

    if options.source_file_policy == SourceFilePolicy::DeleteAfterSuccess {
        io.delete_source(source_rel)?;
    }

    Ok(SplitResult { output_paths })
}

fn map_cue_lib_error(error: CueLibError) -> CueError {
    match error.kind() {
        CueLibErrorKind::ParseError(err) => CueError::Parse {
            line: err.line() + 1,
            column: err.column() + 1,
            message: err.kind().to_string(),
        },
    }
}

fn count_file_commands(cue_text: &str) -> usize {
    cue_text
        .lines()
        .filter(|line| line.trim_start().to_ascii_uppercase().starts_with("FILE "))
        .count()
}

fn parse_rem_fields(cue_text: &str) -> CueRemFields {
    let mut fields = CueRemFields::default();
    let mut current_track: Option<u32> = None;
    for raw in cue_text.lines() {
        let line = raw.trim();
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("TRACK ") {
            current_track = line
                .split_whitespace()
                .nth(1)
                .and_then(|n| n.parse::<u32>().ok());
            continue;
        }
        if !upper.starts_with("REM ") {
            continue;
        }
        let rest = line[4..].trim();
        let Some((key, value)) = split_key_value(rest) else {
            continue;
        };
        if let Some(track_no) = current_track {
            fields.track_extra.push((track_no, key, value));
        } else if key.eq_ignore_ascii_case("DATE") {
            fields.year = value.parse::<u32>().ok();
        } else if key.eq_ignore_ascii_case("GENRE") {
            fields.genre = Some(value);
        } else if key.eq_ignore_ascii_case("COMMENT") {
            fields.comment = Some(value);
        } else {
            fields.album_extra.push((key, value));
        }
    }
    fields
}

fn split_key_value(input: &str) -> Option<(String, String)> {
    let mut parts = input.splitn(2, char::is_whitespace);
    let key = parts.next()?.trim();
    let value = parts.next().unwrap_or("").trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), unquote(value).to_string()))
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(value)
}

fn require_text(issues: &mut Vec<CueIssue>, field: &str, value: &str, code: &str) {
    if value.trim().is_empty() {
        issues.push(issue(
            code,
            &format!("{field} is required"),
            Some(field),
            None,
        ));
    }
}

fn issue(code: &str, message: &str, field: Option<&str>, track_number: Option<u32>) -> CueIssue {
    CueIssue {
        code: code.into(),
        message: message.into(),
        field: field.map(str::to_string),
        track_number,
        line: None,
        column: None,
    }
}

fn parse_timestamp_frames(value: &str) -> Option<u64> {
    let ts = value.parse::<CueTimestamp>().ok()?;
    Some(timestamp_to_frames(&ts))
}

fn timestamp_to_frames(ts: &CueTimestamp) -> u64 {
    (ts.as_duration().as_millis() * 75 / 1000) as u64
}

fn timestamp_to_sample(value: &str, sample_rate: u32) -> Option<usize> {
    let frames = parse_timestamp_frames(value)?;
    Some((frames as u128 * sample_rate as u128 / 75) as usize)
}

fn output_stem(document: &CueDocument, track: &CueTrack, mask: Option<&str>) -> String {
    let default = format!(
        "{:02} - {} - {}",
        track.number,
        track.artist.as_deref().unwrap_or(&document.album_artist),
        track.title
    );
    let raw = match mask {
        Some(mask) if !mask.trim().is_empty() => mask
            .replace("{$n}", &format!("{:02}", track.number))
            .replace("$n", &format!("{:02}", track.number))
            .replace(
                "{$a}",
                track.artist.as_deref().unwrap_or(&document.album_artist),
            )
            .replace(
                "$a",
                track.artist.as_deref().unwrap_or(&document.album_artist),
            )
            .replace("$t", &track.title),
        _ => default,
    };
    sanitize_file_name(&raw)
}

fn sanitize_file_name(name: &str) -> String {
    let out = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>();
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "track".into()
    } else {
        trimmed.into()
    }
}

#[derive(Debug)]
struct FlacMetadataBlock {
    block_type: u8,
    body: Vec<u8>,
}

fn write_track_tags_to_flac_bytes(
    flac: Vec<u8>,
    document: &CueDocument,
    track: &CueTrack,
) -> Result<Vec<u8>> {
    let (mut blocks, audio_start) = read_flac_metadata_blocks(&flac)?;
    upsert_vorbis_comment(&mut blocks, &track_tag_lines(document, track))?;

    let mut out = Vec::with_capacity(flac.len() + 4096);
    out.extend_from_slice(b"fLaC");
    write_flac_metadata_blocks(&mut out, &blocks)?;
    out.extend_from_slice(&flac[audio_start..]);
    Ok(out)
}

fn read_flac_metadata_blocks(flac: &[u8]) -> Result<(Vec<FlacMetadataBlock>, usize)> {
    if flac.get(0..4) != Some(b"fLaC".as_slice()) {
        return Err(CueError::Tags("not a FLAC file".into()));
    }
    let mut pos = 4usize;
    let mut blocks = Vec::new();
    loop {
        if pos + 4 > flac.len() {
            return Err(CueError::Tags("truncated FLAC metadata".into()));
        }
        let header = flac[pos];
        let is_last = (header & 0x80) != 0;
        let block_type = header & 0x7f;
        let length = u32::from_be_bytes([0, flac[pos + 1], flac[pos + 2], flac[pos + 3]]) as usize;
        pos += 4;
        if pos + length > flac.len() {
            return Err(CueError::Tags(format!(
                "metadata block {block_type} overruns file"
            )));
        }
        blocks.push(FlacMetadataBlock {
            block_type,
            body: flac[pos..pos + length].to_vec(),
        });
        pos += length;
        if is_last {
            break;
        }
    }
    if blocks.first().map(|b| b.block_type) != Some(0) {
        return Err(CueError::Tags("missing FLAC STREAMINFO".into()));
    }
    Ok((blocks, pos))
}

fn upsert_vorbis_comment(blocks: &mut Vec<FlacMetadataBlock>, lines: &[String]) -> Result<()> {
    const BLOCK_VORBIS_COMMENT: u8 = 4;
    const BLOCK_PADDING: u8 = 1;
    const PADDING_SIZE: usize = 4096;

    let comment = FlacMetadataBlock {
        block_type: BLOCK_VORBIS_COMMENT,
        body: vorbis_comment_body(lines),
    };
    if let Some(pos) = blocks
        .iter()
        .position(|b| b.block_type == BLOCK_VORBIS_COMMENT)
    {
        blocks[pos] = comment;
        let mut seen = false;
        blocks.retain(|b| {
            if b.block_type != BLOCK_VORBIS_COMMENT {
                return true;
            }
            if seen {
                false
            } else {
                seen = true;
                true
            }
        });
    } else {
        blocks.push(comment);
    }
    if blocks.last().map(|b| b.block_type) == Some(BLOCK_VORBIS_COMMENT) {
        blocks.push(FlacMetadataBlock {
            block_type: BLOCK_PADDING,
            body: vec![0u8; PADDING_SIZE],
        });
    }
    Ok(())
}

fn write_flac_metadata_blocks(out: &mut Vec<u8>, blocks: &[FlacMetadataBlock]) -> Result<()> {
    if blocks.is_empty() {
        return Err(CueError::Tags("missing FLAC metadata".into()));
    }
    for (idx, block) in blocks.iter().enumerate() {
        if block.body.len() > 16_777_215 {
            return Err(CueError::Tags("metadata block too large".into()));
        }
        let is_last = idx + 1 == blocks.len();
        let len = (block.body.len() as u32).to_be_bytes();
        out.push(block.block_type | if is_last { 0x80 } else { 0 });
        out.extend_from_slice(&len[1..4]);
        out.extend_from_slice(&block.body);
    }
    Ok(())
}

fn track_tag_lines(document: &CueDocument, track: &CueTrack) -> Vec<String> {
    let mut lines = Vec::new();
    push_tag_line(&mut lines, "TITLE", &track.title);
    push_tag_line(
        &mut lines,
        "ARTIST",
        track.artist.as_deref().unwrap_or(&document.album_artist),
    );
    push_tag_line(&mut lines, "ALBUM", &document.album_title);
    push_tag_line(&mut lines, "TRACKNUMBER", &track.number.to_string());
    push_tag_line(&mut lines, "TRACKTOTAL", &document.tracks.len().to_string());
    if let Some(year) = document.year {
        push_tag_line(&mut lines, "DATE", &year.to_string());
    }
    if let Some(genre) = track.genre.as_ref().or(document.genre.as_ref()) {
        push_tag_line(&mut lines, "GENRE", genre);
    }
    if let Some(comment) = &document.comment {
        push_tag_line(&mut lines, "COMMENT", comment);
    }
    lines
}

fn push_tag_line(lines: &mut Vec<String>, key: &str, value: &str) {
    if !value.is_empty() {
        lines.push(format!("{key}={value}"));
    }
}

fn vorbis_comment_body(lines: &[String]) -> Vec<u8> {
    let vendor = b"reference libFLAC 1.3.3";
    let mut body = Vec::new();
    body.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    body.extend_from_slice(vendor);
    body.extend_from_slice(&(lines.len() as u32).to_le_bytes());
    for line in lines {
        let bytes = line.as_bytes();
        body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(bytes);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MemorySplitIo {
        sources: BTreeMap<PathBuf, Vec<u8>>,
        outputs: BTreeMap<PathBuf, Vec<u8>>,
        deleted: Vec<PathBuf>,
    }

    impl SplitIo for MemorySplitIo {
        fn read_source(&mut self, audio_path: &Path) -> Result<Vec<u8>> {
            self.sources.get(audio_path).cloned().ok_or_else(|| {
                CueError::Invalid(format!("missing source {}", audio_path.display()))
            })
        }

        fn write_output(&mut self, rel_path: &Path, bytes: Vec<u8>) -> Result<()> {
            self.outputs.insert(rel_path.to_path_buf(), bytes);
            Ok(())
        }

        fn delete_source(&mut self, rel_path: &Path) -> Result<()> {
            self.deleted.push(rel_path.to_path_buf());
            Ok(())
        }
    }

    #[test]
    fn split_flac_image_io_writes_relative_output_paths() {
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
    PERFORMER "Track Artist 2"
    INDEX 01 00:00:37
"#;
        let doc = parse_cue(cue, Path::new("album.cue")).unwrap();
        let mut io = MemorySplitIo::default();
        io.sources.insert(
            PathBuf::from("album.flac"),
            include_bytes!("../../../crates/euterpe-server/tests/fixtures/silent.flac").to_vec(),
        );

        let result = split_flac_image_io(
            &doc,
            &mut io,
            Path::new("split"),
            &SplitOptions {
                source_file_policy: SourceFilePolicy::DeleteAfterSuccess,
                file_mask: None,
                on_progress: None,
            },
        )
        .unwrap();

        assert_eq!(
            result.output_paths,
            vec![
                PathBuf::from("split/01 - Track Artist 1 - First.flac"),
                PathBuf::from("split/02 - Track Artist 2 - Second.flac"),
            ]
        );
        assert!(
            io.outputs
                .contains_key(Path::new("split/01 - Track Artist 1 - First.flac"))
        );
        assert!(
            io.outputs
                .contains_key(Path::new("split/02 - Track Artist 2 - Second.flac"))
        );
        assert_eq!(io.deleted, vec![PathBuf::from("album.flac")]);
    }
}

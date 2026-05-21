use std::io::Cursor;
use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::Accessor;

use crate::error::{ConvertError, Result};

const FLAC_MAGIC: &[u8; 4] = b"fLaC";
const BLOCK_STREAMINFO: u8 = 0;
const BLOCK_VORBIS_COMMENT: u8 = 4;
const BLOCK_PADDING: u8 = 1;
const PADDING_SIZE: u32 = 4096;

#[derive(Default)]
struct TagFields {
    vendor: String,
    lines: Vec<String>,
}

/// Append empty Vorbis Comment + padding after lone STREAMINFO (libflac/xrecode layout).
///
/// Lets tools insert SEEKTABLE before the tail block. Safari misbehaves when SEEKTABLE is
/// the only trailing metadata block.
pub fn ensure_libflac_metadata_tail(flac_path: &Path) -> Result<()> {
    let mut file = std::fs::read(flac_path).map_err(ConvertError::Io)?;
    if file.len() < 42 || file.get(0..4) != Some(FLAC_MAGIC) {
        return Err(ConvertError::Tags("not a FLAC file".into()));
    }
    let header = file[4];
    let is_last = (header & 0x80) != 0;
    let block_type = header & 0x7f;
    if !is_last || block_type != BLOCK_STREAMINFO {
        return Ok(());
    }

    let fields = TagFields {
        vendor: "reference libFLAC 1.3.3".to_string(),
        lines: Vec::new(),
    };
    insert_vorbis_comment_tail(&mut file, &fields)?;
    std::fs::write(flac_path, file).map_err(ConvertError::Io)?;
    Ok(())
}

/// Insert Vorbis Comment as the last metadata block (no extra PADDING; matches libflac exports).
fn insert_vorbis_comment_tail(file: &mut Vec<u8>, fields: &TagFields) -> Result<()> {
    if file.len() < 8 || file.get(0..4) != Some(FLAC_MAGIC) {
        return Err(ConvertError::Tags("not a FLAC file".into()));
    }

    let meta_end = metadata_end_offset(file)?;
    let mut body = Cursor::new(Vec::new());
    write_vorbis_comment_body(&mut body, fields)?;
    let body = body.into_inner();
    let comment = build_metadata_block(BLOCK_VORBIS_COMMENT, body.len() as u32, true, &body)?;

    file[4] &= 0x7f;
    file.splice(meta_end..meta_end, comment);
    Ok(())
}

pub fn transfer_tags(src: &Path, dst_flac: &Path) -> Result<()> {
    let fields = read_tag_fields(src)?;
    if fields.lines.is_empty() {
        return Ok(());
    }

    let mut file = std::fs::read(dst_flac).map_err(ConvertError::Io)?;
    insert_vorbis_comment_block(&mut file, &fields)?;
    std::fs::write(dst_flac, file).map_err(ConvertError::Io)?;
    Ok(())
}

fn read_tag_fields(src: &Path) -> Result<TagFields> {
    let tagged = Probe::open(src)
        .map_err(|e| ConvertError::Tags(e.to_string()))?
        .guess_file_type()
        .map_err(|e| ConvertError::Tags(e.to_string()))?
        .read()
        .map_err(|e| ConvertError::Tags(e.to_string()))?;

    let Some(tag) = tagged.primary_tag().or_else(|| tagged.tags().first()) else {
        return Ok(TagFields::default());
    };

    let mut fields = TagFields {
        vendor: "reference libFLAC 1.3.3".to_string(),
        lines: Vec::new(),
    };

    let mut push = |key: &str, value: &str| {
        if !value.is_empty() {
            fields.lines.push(format!("{key}={value}"));
        }
    };

    if let Some(v) = tag.title() {
        push("TITLE", v.as_ref());
    }
    if let Some(v) = tag.artist() {
        push("ARTIST", v.as_ref());
    }
    if let Some(v) = tag.album() {
        push("ALBUM", v.as_ref());
    }
    if let Some(v) = tag.year() {
        push("DATE", &v.to_string());
    }
    if let Some(n) = tag.track() {
        push("TRACKNUMBER", &n.to_string());
    }
    if let Some(n) = tag.track_total() {
        push("TRACKTOTAL", &n.to_string());
    }
    if let Some(n) = tag.disk() {
        push("DISCNUMBER", &n.to_string());
    }
    if let Some(n) = tag.disk_total() {
        push("DISCTOTAL", &n.to_string());
    }
    if let Some(v) = tag.genre() {
        push("GENRE", v.as_ref());
    }
    if let Some(v) = tag.comment() {
        push("COMMENT", v.as_ref());
    }
    if let Some(v) = tag.get_string(&ItemKey::Label) {
        push("LABEL", v);
    }
    if let Some(v) = tag.get_string(&ItemKey::Isrc) {
        push("ISRC", v);
    }
    if let Some(v) = tag.get_string(&ItemKey::Composer) {
        push("COMPOSER", v);
    }

    Ok(fields)
}

fn insert_vorbis_comment_block(file: &mut Vec<u8>, fields: &TagFields) -> Result<()> {
    if file.len() < 8 || file.get(0..4) != Some(FLAC_MAGIC) {
        return Err(ConvertError::Tags("not a FLAC file".into()));
    }

    let meta_end = metadata_end_offset(file)?;
    let comment = build_vorbis_comment_block(fields)?;
    let padding = build_metadata_block(BLOCK_PADDING, PADDING_SIZE, true, &[])?;

    // STREAMINFO was written as the last metadata block; clear the flag and insert tags.
    file[4] &= 0x7f;

    let mut insert = comment;
    insert.extend_from_slice(&padding);
    file.splice(meta_end..meta_end, insert);
    // Note: tag transfer keeps PADDING last so later SEEKTABLE can be inserted before it.

    Ok(())
}

fn metadata_end_offset(file: &[u8]) -> Result<usize> {
    let mut pos = 4usize;
    loop {
        if pos + 4 > file.len() {
            return Err(ConvertError::Tags("truncated FLAC metadata".into()));
        }
        let header = file[pos];
        let is_last = (header & 0x80) != 0;
        let block_type = header & 0x7f;
        let length = u32::from_be_bytes([0, file[pos + 1], file[pos + 2], file[pos + 3]]) as usize;
        pos += 4;
        if pos + length > file.len() {
            return Err(ConvertError::Tags(format!(
                "metadata block {block_type} overruns file"
            )));
        }
        pos += length;
        if is_last {
            return Ok(pos);
        }
    }
}

fn build_vorbis_comment_block(fields: &TagFields) -> Result<Vec<u8>> {
    let mut body = Cursor::new(Vec::new());
    write_vorbis_comment_body(&mut body, fields)?;
    let body = body.into_inner();
    build_metadata_block(
        BLOCK_VORBIS_COMMENT,
        body.len() as u32,
        false,
        &body,
    )
}

fn build_metadata_block(
    block_type: u8,
    length: u32,
    is_last: bool,
    body: &[u8],
) -> Result<Vec<u8>> {
    if length > 16_777_215 {
        return Err(ConvertError::Tags("metadata block too large".into()));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    let mut header = block_type;
    if is_last {
        header |= 0x80;
    }
    out.push(header);
    out.extend_from_slice(&length.to_be_bytes()[1..]);
    out.extend_from_slice(body);
    Ok(out)
}

fn write_vorbis_comment_body(w: &mut Cursor<Vec<u8>>, fields: &TagFields) -> Result<()> {
    let vendor = fields.vendor.as_bytes();
    write_u32_le(w, vendor.len() as u32);
    w.get_mut().extend_from_slice(vendor);

    write_u32_le(w, fields.lines.len() as u32);
    for line in &fields.lines {
        let bytes = line.as_bytes();
        write_u32_le(w, bytes.len() as u32);
        w.get_mut().extend_from_slice(bytes);
    }
    Ok(())
}

fn write_u32_le(w: &mut Cursor<Vec<u8>>, v: u32) {
    w.get_mut().extend_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::streaming::encode_flac_streaming;
    use crate::settings::FlacEncodeSettings;
    use flacenc::source::MemSource;

    #[test]
    fn ensure_libflac_metadata_tail_adds_vorbis_last() {
        let samples: Vec<i32> = (0..4000).map(|i| i % 500).collect();
        let mut flac = Vec::new();
        let src = MemSource::from_samples(&samples, 2, 16, 44_100);
        encode_flac_streaming(src, &FlacEncodeSettings::default(), &mut flac, None).unwrap();
        assert_eq!(flac[4] & 0x80, 0x80, "flacenc leaves STREAMINFO as last");

        let path = std::env::temp_dir().join(format!("euterpe-flac-tail-{}.flac", std::process::id()));
        std::fs::write(&path, &flac).unwrap();
        ensure_libflac_metadata_tail(&path).unwrap();
        let updated = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(updated[4] & 0x80, 0, "STREAMINFO must not be last");
        assert_eq!(updated[4] & 0x7f, 0, "first block still STREAMINFO");

        crate::encode::decode_flac_bytes(&updated).expect("claxon decodes tail metadata");
    }

    #[test]
    fn transfer_tags_does_not_truncate_flac() {
        let samples: Vec<i32> = (0..8000).map(|i| (i % 1000) as i32 - 500).collect();
        let mut flac = Vec::new();
        let src = MemSource::from_samples(&samples, 2, 16, 44_100);
        encode_flac_streaming(src, &FlacEncodeSettings::default(), &mut flac, None).unwrap();
        let size_before = flac.len();
        assert!(size_before > 1000);

        // Insert tags in-place on encoded FLAC:
        // call insert directly
        let fields = TagFields {
            vendor: "test".into(),
            lines: vec!["ARTIST=Genesis".into()],
        };
        insert_vorbis_comment_block(&mut flac, &fields).unwrap();
        assert!(flac.len() > size_before);
        assert_eq!(&flac[0..4], b"fLaC");
    }
}

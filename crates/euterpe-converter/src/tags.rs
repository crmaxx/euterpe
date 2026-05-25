use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Cursor, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

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
static TAG_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct TagFields {
    vendor: String,
    lines: Vec<String>,
}

struct MetadataBlock {
    block_type: u8,
    body: Vec<u8>,
}

/// Append empty Vorbis Comment + padding after lone STREAMINFO (libflac/xrecode layout).
///
/// Lets tools insert SEEKTABLE before the tail block. Safari misbehaves when SEEKTABLE is
/// the only trailing metadata block.
pub fn ensure_libflac_metadata_tail(flac_path: &Path) -> Result<()> {
    rewrite_flac_metadata(flac_path, |blocks| {
        if blocks.len() == 1 && blocks[0].block_type == BLOCK_STREAMINFO {
            let fields = TagFields {
                vendor: "reference libFLAC 1.3.3".to_string(),
                lines: Vec::new(),
            };
            blocks.push(MetadataBlock {
                block_type: BLOCK_VORBIS_COMMENT,
                body: vorbis_comment_body(&fields)?,
            });
        }
        Ok(())
    })
}

/// Insert Vorbis Comment as the last metadata block (no extra PADDING; matches libflac exports).
#[cfg(test)]
fn insert_vorbis_comment_tail(file: &mut Vec<u8>, fields: &TagFields) -> Result<()> {
    if file.len() < 8 || file.get(0..4) != Some(FLAC_MAGIC) {
        return Err(ConvertError::Tags("not a FLAC file".into()));
    }

    let (last_header, meta_end) = metadata_last_block(file)?;
    let mut body = Cursor::new(Vec::new());
    write_vorbis_comment_body(&mut body, fields)?;
    let body = body.into_inner();
    let comment = build_metadata_block(BLOCK_VORBIS_COMMENT, body.len() as u32, true, &body)?;

    file[last_header] &= 0x7f;
    file.splice(meta_end..meta_end, comment);
    Ok(())
}

pub fn transfer_tags(src: &Path, dst_flac: &Path) -> Result<()> {
    let fields = read_tag_fields(src)?;
    if fields.lines.is_empty() {
        return Ok(());
    }

    rewrite_flac_metadata(dst_flac, |blocks| upsert_vorbis_comment(blocks, &fields))
}

fn rewrite_flac_metadata(
    flac_path: &Path,
    transform: impl FnOnce(&mut Vec<MetadataBlock>) -> Result<()>,
) -> Result<()> {
    let input = File::open(flac_path).map_err(ConvertError::Io)?;
    let mut reader = BufReader::new(input);
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic).map_err(ConvertError::Io)?;
    if &magic != FLAC_MAGIC {
        return Err(ConvertError::Tags("not a FLAC file".into()));
    }

    let mut blocks = read_metadata_blocks(&mut reader)?;
    transform(&mut blocks)?;

    let parent = flac_path
        .parent()
        .ok_or_else(|| ConvertError::Io(io::Error::other("no parent")))?;
    let (temp_path, temp_file) = create_temp_file(parent)?;
    let result = (|| -> Result<()> {
        let mut writer = BufWriter::new(temp_file);
        writer.write_all(FLAC_MAGIC).map_err(ConvertError::Io)?;
        write_metadata_blocks(&mut writer, &blocks)?;
        io::copy(&mut reader, &mut writer).map_err(ConvertError::Io)?;
        let file = writer
            .into_inner()
            .map_err(|e| ConvertError::Io(e.into_error()))?;
        file.sync_all().map_err(ConvertError::Io)?;
        fs::rename(&temp_path, flac_path).map_err(ConvertError::Io)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn create_temp_file(parent: &Path) -> Result<(std::path::PathBuf, File)> {
    loop {
        let n = TAG_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!(".euterpe-tags-{}-{n}.flac.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(ConvertError::Io(e)),
        }
    }
}

fn read_metadata_blocks(reader: &mut impl Read) -> Result<Vec<MetadataBlock>> {
    let mut blocks = Vec::new();
    loop {
        let mut header = [0u8; 4];
        reader.read_exact(&mut header).map_err(ConvertError::Io)?;
        let is_last = (header[0] & 0x80) != 0;
        let block_type = header[0] & 0x7f;
        let length = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
        let mut body = vec![0u8; length];
        reader.read_exact(&mut body).map_err(ConvertError::Io)?;
        blocks.push(MetadataBlock { block_type, body });
        if is_last {
            break;
        }
    }
    if blocks.first().map(|b| b.block_type) != Some(BLOCK_STREAMINFO) {
        return Err(ConvertError::Tags("missing FLAC STREAMINFO".into()));
    }
    Ok(blocks)
}

fn write_metadata_blocks(writer: &mut impl Write, blocks: &[MetadataBlock]) -> Result<()> {
    if blocks.is_empty() {
        return Err(ConvertError::Tags("missing FLAC metadata".into()));
    }
    for (i, block) in blocks.iter().enumerate() {
        let is_last = i + 1 == blocks.len();
        let header = metadata_header(block.block_type, block.body.len() as u32, is_last)?;
        writer.write_all(&header).map_err(ConvertError::Io)?;
        writer.write_all(&block.body).map_err(ConvertError::Io)?;
    }
    Ok(())
}

fn upsert_vorbis_comment(blocks: &mut Vec<MetadataBlock>, fields: &TagFields) -> Result<()> {
    let comment = MetadataBlock {
        block_type: BLOCK_VORBIS_COMMENT,
        body: vorbis_comment_body(fields)?,
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
        if blocks.last().map(|b| b.block_type) == Some(BLOCK_VORBIS_COMMENT) {
            blocks.push(MetadataBlock {
                block_type: BLOCK_PADDING,
                body: vec![0u8; PADDING_SIZE as usize],
            });
        }
        return Ok(());
    }

    blocks.push(comment);
    blocks.push(MetadataBlock {
        block_type: BLOCK_PADDING,
        body: vec![0u8; PADDING_SIZE as usize],
    });
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

#[cfg(test)]
fn insert_vorbis_comment_block(file: &mut Vec<u8>, fields: &TagFields) -> Result<()> {
    if file.len() < 8 || file.get(0..4) != Some(FLAC_MAGIC) {
        return Err(ConvertError::Tags("not a FLAC file".into()));
    }

    let comment = build_vorbis_comment_block(fields)?;
    let padding = build_metadata_block(BLOCK_PADDING, PADDING_SIZE, true, &[])?;

    if let Some((start, end, is_last)) = metadata_block(file, BLOCK_VORBIS_COMMENT)? {
        let mut replacement = comment;
        if is_last {
            replacement.extend_from_slice(&padding);
        }
        file.splice(start..end, replacement);
        return Ok(());
    }

    let (last_header, meta_end) = metadata_last_block(file)?;

    // Clear the current LAST flag, whichever metadata block currently carries it.
    file[last_header] &= 0x7f;

    let mut insert = comment;
    insert.extend_from_slice(&padding);
    file.splice(meta_end..meta_end, insert);
    // Note: tag transfer keeps PADDING last so later SEEKTABLE can be inserted before it.

    Ok(())
}

#[cfg(test)]
fn metadata_block(file: &[u8], expected_type: u8) -> Result<Option<(usize, usize, bool)>> {
    let mut pos = 4usize;
    loop {
        if pos + 4 > file.len() {
            return Err(ConvertError::Tags("truncated FLAC metadata".into()));
        }
        let header_pos = pos;
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
        if block_type == expected_type {
            return Ok(Some((header_pos, pos, is_last)));
        }
        if is_last {
            return Ok(None);
        }
    }
}

#[cfg(test)]
fn metadata_last_block(file: &[u8]) -> Result<(usize, usize)> {
    let mut pos = 4usize;
    loop {
        if pos + 4 > file.len() {
            return Err(ConvertError::Tags("truncated FLAC metadata".into()));
        }
        let header_pos = pos;
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
            return Ok((header_pos, pos));
        }
    }
}

#[cfg(test)]
fn build_vorbis_comment_block(fields: &TagFields) -> Result<Vec<u8>> {
    let mut body = Cursor::new(Vec::new());
    write_vorbis_comment_body(&mut body, fields)?;
    let body = body.into_inner();
    build_metadata_block(BLOCK_VORBIS_COMMENT, body.len() as u32, false, &body)
}

#[cfg(test)]
fn build_metadata_block(
    block_type: u8,
    length: u32,
    is_last: bool,
    body: &[u8],
) -> Result<Vec<u8>> {
    let header = metadata_header(block_type, length, is_last)?;
    let length = length as usize;
    if !body.is_empty() && body.len() != length {
        return Err(ConvertError::Tags("metadata block length mismatch".into()));
    }

    let mut out = Vec::with_capacity(4 + length);
    out.extend_from_slice(&header);
    if body.is_empty() {
        out.resize(4 + length, 0);
    } else {
        out.extend_from_slice(body);
    }
    Ok(out)
}

fn metadata_header(block_type: u8, length: u32, is_last: bool) -> Result<[u8; 4]> {
    if length > 16_777_215 {
        return Err(ConvertError::Tags("metadata block too large".into()));
    }
    let mut header = block_type;
    if is_last {
        header |= 0x80;
    }
    let len = length.to_be_bytes();
    Ok([header, len[1], len[2], len[3]])
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

fn vorbis_comment_body(fields: &TagFields) -> Result<Vec<u8>> {
    let mut body = Cursor::new(Vec::new());
    write_vorbis_comment_body(&mut body, fields)?;
    Ok(body.into_inner())
}

fn write_u32_le(w: &mut Cursor<Vec<u8>>, v: u32) {
    w.get_mut().extend_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::libflac::encode_flac_with_libflac;
    use crate::settings::FlacEncodeSettings;
    use crate::source::traits::{Fill, PcmRead};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestPcmSource {
        samples: Vec<i32>,
        pos: usize,
    }

    impl TestPcmSource {
        fn new(samples: Vec<i32>) -> Self {
            Self { samples, pos: 0 }
        }
    }

    impl PcmRead for TestPcmSource {
        fn channels(&self) -> usize {
            2
        }

        fn bits_per_sample(&self) -> usize {
            16
        }

        fn sample_rate(&self) -> usize {
            44_100
        }

        fn len_hint(&self) -> Option<usize> {
            Some(self.samples.len() / 2)
        }

        fn read_samples<F: Fill>(
            &mut self,
            block_size: usize,
            dest: &mut F,
        ) -> crate::error::Result<usize> {
            let frame_pos = self.pos / 2;
            let frames_left = self.samples.len() / 2 - frame_pos;
            let frames = block_size.min(frames_left);
            if frames == 0 {
                return Ok(0);
            }
            let end = self.pos + frames * 2;
            dest.fill_interleaved(&self.samples[self.pos..end])?;
            self.pos = end;
            Ok(frames)
        }
    }

    fn encode_test_flac(samples: Vec<i32>) -> Vec<u8> {
        let n = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "euterpe-test-flac-{}-{n}-{}.flac",
            std::process::id(),
            samples.len()
        ));
        encode_flac_with_libflac(
            TestPcmSource::new(samples),
            &path,
            &FlacEncodeSettings::default(),
            None,
        )
        .unwrap();
        let data = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(path);
        data
    }

    fn keep_only_streaminfo_metadata(mut flac: Vec<u8>) -> Vec<u8> {
        let blocks = metadata_blocks(&flac);
        let streaminfo_end = blocks[0].3;
        let frames_start = blocks.last().unwrap().3;
        let frames = flac.split_off(frames_start);
        flac.truncate(streaminfo_end);
        flac[4] |= 0x80;
        flac.extend_from_slice(&frames);
        flac
    }

    fn metadata_blocks(file: &[u8]) -> Vec<(u8, bool, usize, usize)> {
        assert_eq!(file.get(0..4), Some(FLAC_MAGIC.as_slice()));
        let mut pos = 4usize;
        let mut out = Vec::new();
        loop {
            let header = file[pos];
            let is_last = (header & 0x80) != 0;
            let block_type = header & 0x7f;
            let length =
                u32::from_be_bytes([0, file[pos + 1], file[pos + 2], file[pos + 3]]) as usize;
            let start = pos;
            pos += 4 + length;
            out.push((block_type, is_last, start, pos));
            if is_last {
                break;
            }
        }
        out
    }

    #[test]
    fn ensure_libflac_metadata_tail_adds_vorbis_last() {
        let samples: Vec<i32> = (0..4000).map(|i| i % 500).collect();
        let flac = keep_only_streaminfo_metadata(encode_test_flac(samples));
        assert_eq!(flac[4] & 0x80, 0x80, "fixture leaves STREAMINFO as last");

        let path =
            std::env::temp_dir().join(format!("euterpe-flac-tail-{}.flac", std::process::id()));
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
        let samples: Vec<i32> = (0..8000).map(|i| (i % 1000) - 500).collect();
        let mut flac = encode_test_flac(samples);
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

    #[test]
    fn transfer_tags_after_empty_tail_keeps_single_last_metadata_block() {
        let samples: Vec<i32> = (0..8000).map(|i| (i % 1000) - 500).collect();
        let mut flac = keep_only_streaminfo_metadata(encode_test_flac(samples));
        let empty = TagFields {
            vendor: "test".into(),
            lines: Vec::new(),
        };
        let tagged = TagFields {
            vendor: "test".into(),
            lines: vec!["ARTIST=Genesis".into()],
        };

        insert_vorbis_comment_tail(&mut flac, &empty).unwrap();
        insert_vorbis_comment_block(&mut flac, &tagged).unwrap();

        let blocks = metadata_blocks(&flac);
        assert_eq!(
            blocks.iter().filter(|(_, last, _, _)| *last).count(),
            1,
            "FLAC metadata chain must have exactly one LAST block"
        );
        assert!(blocks.iter().any(|(t, _, _, _)| *t == BLOCK_VORBIS_COMMENT));
        crate::encode::decode_flac_bytes(&flac).expect("strict metadata chain decodes");
    }
}

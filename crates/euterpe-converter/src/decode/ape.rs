use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use ape_decoder::ApeDecoder;

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;

pub fn decode(path: &Path) -> Result<PcmBuffer> {
    let file = File::open(path).map_err(ConvertError::Io)?;
    let mut decoder = ApeDecoder::new(BufReader::new(file))
        .map_err(|e| ConvertError::Decode(e.to_string()))?;
    let info = decoder.info();
    let bits = info.bits_per_sample;
    let channels = info.channels as u8;
    let sample_rate = info.sample_rate;
    let bytes_per_sample = (bits / 8) as usize;

    let pcm_bytes = decoder
        .decode_all()
        .map_err(|e| ConvertError::Decode(e.to_string()))?;

    if bytes_per_sample == 0 || bits > 32 {
        return Err(ConvertError::Decode(format!(
            "unsupported APE bit depth: {bits}"
        )));
    }

    let frame_bytes = bytes_per_sample * channels as usize;
    if frame_bytes == 0 {
        return Err(ConvertError::Decode("invalid channel count".into()));
    }
    if !pcm_bytes.len().is_multiple_of(frame_bytes) {
        return Err(ConvertError::Decode("APE PCM length mismatch".into()));
    }

    let mut samples = Vec::with_capacity(pcm_bytes.len() / bytes_per_sample);
    for chunk in pcm_bytes.chunks_exact(frame_bytes) {
        for ch in 0..channels as usize {
            let off = ch * bytes_per_sample;
            let s = match bytes_per_sample {
                1 => i32::from(chunk[off] as i8),
                2 => i32::from(i16::from_le_bytes([chunk[off], chunk[off + 1]])),
                3 => {
                    let b0 = chunk[off] as i32;
                    let b1 = chunk[off + 1] as i32;
                    let b2 = chunk[off + 2] as i32;
                    let v = b0 | (b1 << 8) | (b2 << 16);
                    if v & 0x80_00_00 != 0 {
                        v | !0xFF_FF_FF
                    } else {
                        v
                    }
                }
                4 => i32::from_le_bytes([
                    chunk[off],
                    chunk[off + 1],
                    chunk[off + 2],
                    chunk[off + 3],
                ]),
                _ => unreachable!(),
            };
            samples.push(s);
        }
    }

    Ok(PcmBuffer {
        samples,
        channels,
        bits_per_sample: bits as u8,
        sample_rate,
    })
}

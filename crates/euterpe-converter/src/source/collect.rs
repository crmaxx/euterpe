use crate::error::{ConvertError, Result};
use crate::pcm_push::convert_sample;
use crate::source::traits::Fill;

fn decode_err(msg: String) -> ConvertError {
    ConvertError::Decode(msg)
}

/// Append PCM into a `Vec<i32>` for full-buffer test decode paths.
pub struct VecFill {
    pub samples: Vec<i32>,
    pub channels: usize,
    pub bits_per_sample: u8,
}

impl Fill for VecFill {
    fn fill_interleaved(&mut self, interleaved: &[i32]) -> Result<()> {
        // AlacSource already bit-converts; extend as-is (clamp is a no-op in range).
        self.samples.extend_from_slice(interleaved);
        Ok(())
    }

    fn fill_le_bytes(&mut self, bytes: &[u8], bytes_per_sample: usize) -> Result<()> {
        let frame_bytes = bytes_per_sample * self.channels;
        if frame_bytes == 0 || !bytes.len().is_multiple_of(frame_bytes) {
            return Err(decode_err("PCM byte length mismatch".into()));
        }
        let bits = self.bits_per_sample;
        for chunk in bytes.chunks_exact(frame_bytes) {
            for ch in 0..self.channels {
                let off = ch * bytes_per_sample;
                let s = match bytes_per_sample {
                    1 => convert_sample(i32::from(chunk[off] as i8), 8, bits),
                    2 => convert_sample(
                        i32::from(i16::from_le_bytes([chunk[off], chunk[off + 1]])),
                        16,
                        bits,
                    ),
                    3 => {
                        let b0 = chunk[off] as i32;
                        let b1 = chunk[off + 1] as i32;
                        let b2 = chunk[off + 2] as i32;
                        let v = b0 | (b1 << 8) | (b2 << 16);
                        let v = if v & 0x80_00_00 != 0 {
                            v | !0xFF_FF_FF
                        } else {
                            v
                        };
                        convert_sample(v, 24, bits)
                    }
                    4 => convert_sample(
                        i32::from_le_bytes([
                            chunk[off],
                            chunk[off + 1],
                            chunk[off + 2],
                            chunk[off + 3],
                        ]),
                        32,
                        bits,
                    ),
                    _ => {
                        return Err(decode_err(format!(
                            "unsupported bytes_per_sample: {bytes_per_sample}"
                        )));
                    }
                };
                self.samples.push(s);
            }
        }
        Ok(())
    }
}

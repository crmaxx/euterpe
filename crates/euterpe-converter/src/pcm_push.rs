//! Push decoded audio buffers into an interleaved `Vec<i32>` (shared by PCM sources).

use symphonia::core::audio::{AudioBufferRef, Signal};

pub fn append_symphonia_buffer(buf: AudioBufferRef<'_>, target_bits: u8, out: &mut Vec<i32>) {
    let source_bits = symphonia_buffer_bits(&buf);
    match buf {
        AudioBufferRef::F32(b) => {
            let peak = (1i32 << target_bits.saturating_sub(1)) as f32;
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    let s = b.chan(c)[f];
                    let v = (s * peak).round() as i32;
                    out.push(clamp_sample(v, target_bits));
                }
            }
        }
        AudioBufferRef::S16(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    out.push(convert_sample(
                        i32::from(b.chan(c)[f]),
                        source_bits,
                        target_bits,
                    ));
                }
            }
        }
        AudioBufferRef::S24(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    out.push(convert_sample(
                        b.chan(c)[f].inner(),
                        source_bits,
                        target_bits,
                    ));
                }
            }
        }
        AudioBufferRef::S32(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    out.push(convert_sample(b.chan(c)[f], source_bits, target_bits));
                }
            }
        }
        AudioBufferRef::U8(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    let u = b.chan(c)[f];
                    out.push(convert_sample(
                        (i32::from(u) - 128) << 8,
                        source_bits,
                        target_bits,
                    ));
                }
            }
        }
        _ => {}
    }
}

fn symphonia_buffer_bits(buf: &AudioBufferRef<'_>) -> u8 {
    match buf {
        AudioBufferRef::S16(_) => 16,
        AudioBufferRef::S24(_) => 24,
        AudioBufferRef::S32(_) => 32,
        AudioBufferRef::U8(_) => 8,
        AudioBufferRef::F32(_) => 32,
        _ => 16,
    }
}

/// Reduce or expand integer PCM between bit depths (shift, not clip).
pub fn convert_sample(value: i32, source_bits: u8, target_bits: u8) -> i32 {
    if source_bits == 0 || target_bits == 0 {
        return 0;
    }
    let v = if source_bits > target_bits {
        value >> (source_bits - target_bits)
    } else if source_bits < target_bits {
        value << (target_bits - source_bits)
    } else {
        value
    };
    clamp_sample(v, target_bits)
}

pub fn clamp_sample(s: i32, bits_per_sample: u8) -> i32 {
    if bits_per_sample >= 32 {
        return s;
    }
    let bits = bits_per_sample.max(1) as u32;
    let max = ((1i64 << (bits - 1)) - 1) as i32;
    let min = (-(1i64 << (bits - 1))) as i32;
    s.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::{clamp_sample, convert_sample};

    #[test]
    fn s24_to_16_shifts_not_clips() {
        let s24 = 1_000_000i32;
        assert_eq!(convert_sample(s24, 24, 16), s24 >> 8);
        assert_eq!(convert_sample(8_388_607, 24, 16), 32_767);
    }

    #[test]
    fn s16_to_16_unchanged() {
        assert_eq!(convert_sample(-12_345, 16, 16), -12_345);
    }

    #[test]
    fn clamp_only_at_target_ceiling() {
        assert_eq!(clamp_sample(50_000, 16), 32_767);
    }

    #[test]
    fn clamp_32_bit_keeps_full_i32_range() {
        assert_eq!(clamp_sample(i32::MAX, 32), i32::MAX);
        assert_eq!(clamp_sample(i32::MIN, 32), i32::MIN);
    }
}

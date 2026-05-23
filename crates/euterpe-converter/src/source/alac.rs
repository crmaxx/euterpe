use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::{CODEC_TYPE_ALAC, Decoder, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::FormatReader;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;
use crate::pcm_push::append_symphonia_buffer;
use crate::source::traits::{Fill, PcmRead};

type BoxedDecoder = Box<dyn Decoder>;

/// Parsed ALAC "magic cookie" (MP4 `alac` atom extra data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AlacMagicCookie {
    bit_depth: u8,
    sample_rate: u32,
    num_channels: u8,
}

/// Read stream format from the ALAC magic cookie (24-byte payload, or payload embedded in MP4 extra data).
/// Layout matches [symphonia-codec-alac](https://docs.rs/symphonia-codec-alac).
fn parse_alac_magic_cookie(data: &[u8]) -> Option<AlacMagicCookie> {
    fn parse_payload(data: &[u8]) -> Option<AlacMagicCookie> {
        if data.len() < 24 {
            return None;
        }
        let bit_depth = *data.get(5)?;
        if !(1..=32).contains(&bit_depth) {
            return None;
        }
        let num_channels = *data.get(9)?;
        if !(1..=8).contains(&num_channels) {
            return None;
        }
        let sample_rate = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        if sample_rate == 0 {
            return None;
        }
        Some(AlacMagicCookie {
            bit_depth,
            sample_rate,
            num_channels,
        })
    }

    parse_payload(data)
        .or_else(|| data.get(12..).and_then(parse_payload))
        .or_else(|| data.get(24..).and_then(parse_payload))
}

fn resolve_alac_format(params: &symphonia::core::codecs::CodecParameters) -> (usize, usize, usize) {
    let cookie = params
        .extra_data
        .as_deref()
        .and_then(parse_alac_magic_cookie);

    let channels = params
        .channels
        .map(|c| c.count())
        .or_else(|| cookie.map(|c| c.num_channels as usize))
        .unwrap_or(2);

    let bits_per_sample = params
        .bits_per_sample
        .or(params.bits_per_coded_sample)
        .map(|b| b as usize)
        .or_else(|| cookie.map(|c| c.bit_depth as usize))
        .unwrap_or(16);

    let sample_rate = params
        .sample_rate
        .map(|r| r as usize)
        .or_else(|| cookie.map(|c| c.sample_rate as usize))
        .unwrap_or(44_100);

    (sample_rate, channels, bits_per_sample)
}

pub struct AlacSource {
    format: Box<dyn FormatReader>,
    decoder: BoxedDecoder,
    track_id: u32,
    channels: usize,
    bits_per_sample: usize,
    sample_rate: usize,
    total_samples: Option<usize>,
    pending: Vec<i32>,
}

impl AlacSource {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(ConvertError::Io)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| ConvertError::Decode(e.to_string()))?;

        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec == CODEC_TYPE_ALAC)
            .ok_or(ConvertError::NotAlac)?;
        let track_id = track.id;

        let (sample_rate, channels, bits_per_sample) = resolve_alac_format(&track.codec_params);

        tracing::info!(
            path = %path.display(),
            sample_rate,
            bits_per_sample,
            channels,
            "alac stream format"
        );

        let total_samples = track.codec_params.n_frames.map(|f| f as usize);

        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| ConvertError::Decode(e.to_string()))?;

        Ok(Self {
            format,
            decoder,
            track_id,
            channels,
            bits_per_sample,
            sample_rate,
            total_samples,
            pending: Vec::new(),
        })
    }

    fn decode_more(&mut self) -> Result<bool> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(Error::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(Error::IoError(_)) => return Ok(false),
                Err(e) => {
                    return Err(decode_err(e.to_string()));
                }
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            let decoded = self
                .decoder
                .decode(&packet)
                .map_err(|e| decode_err(e.to_string()))?;
            append_symphonia_buffer(decoded, self.bits_per_sample as u8, &mut self.pending);
            return Ok(true);
        }
    }
}

fn decode_err(msg: String) -> ConvertError {
    ConvertError::Decode(msg)
}

impl PcmRead for AlacSource {
    fn channels(&self) -> usize {
        self.channels
    }

    fn bits_per_sample(&self) -> usize {
        self.bits_per_sample
    }

    fn sample_rate(&self) -> usize {
        self.sample_rate
    }

    fn len_hint(&self) -> Option<usize> {
        self.total_samples
    }

    fn read_samples<F: Fill>(&mut self, block_size: usize, dest: &mut F) -> Result<usize> {
        loop {
            let available = self.pending.len() / self.channels;
            if available >= block_size {
                break;
            }
            if !self.decode_more()? {
                break;
            }
        }

        let available = self.pending.len() / self.channels;
        if available == 0 {
            return Ok(0);
        }

        let frame_samples = block_size.min(available);
        let take = frame_samples * self.channels;
        dest.fill_interleaved(&self.pending[..take])?;
        self.pending.drain(..take);
        Ok(frame_samples)
    }
}

/// Full-buffer decode for tests and `decode_to_pcm`.
pub fn decode(path: &Path) -> Result<PcmBuffer> {
    let mut src = AlacSource::open(path)?;
    let mut fill = crate::source::collect::VecFill {
        samples: Vec::new(),
        channels: src.channels,
        bits_per_sample: src.bits_per_sample as u8,
    };
    let block = 4096usize;
    loop {
        let n = src.read_samples(block, &mut fill)?;
        if n == 0 {
            break;
        }
    }
    if fill.samples.is_empty() {
        return Err(ConvertError::Decode("no audio decoded".into()));
    }
    Ok(PcmBuffer {
        samples: fill.samples,
        channels: src.channels as u8,
        bits_per_sample: src.bits_per_sample as u8,
        sample_rate: src.sample_rate as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie_bytes(bit_depth: u8, sample_rate: u32, channels: u8) -> [u8; 24] {
        let mut b = [0u8; 24];
        b[5] = bit_depth;
        b[9] = channels;
        b[20..24].copy_from_slice(&sample_rate.to_be_bytes());
        b
    }

    fn cookie_bytes_36(bit_depth: u8, sample_rate: u32, channels: u8) -> [u8; 36] {
        let mut b = [0u8; 36];
        b[17] = bit_depth;
        b[21] = channels;
        b[32..36].copy_from_slice(&sample_rate.to_be_bytes());
        b
    }

    #[test]
    fn parse_alac_magic_cookie_24bit_48k() {
        let data = cookie_bytes(24, 48_000, 2);
        let c = parse_alac_magic_cookie(&data).unwrap();
        assert_eq!(c.bit_depth, 24);
        assert_eq!(c.sample_rate, 48_000);
        assert_eq!(c.num_channels, 2);
    }

    #[test]
    fn resolve_prefers_cookie_when_params_missing() {
        let mut params = symphonia::core::codecs::CodecParameters::new();
        params
            .extra_data
            .replace(cookie_bytes(24, 48_000, 2).to_vec().into_boxed_slice());
        let (rate, ch, bits) = resolve_alac_format(&params);
        assert_eq!(rate, 48_000);
        assert_eq!(ch, 2);
        assert_eq!(bits, 24);
    }

    #[test]
    fn parse_alac_magic_cookie_36byte_isomp4_extra_data() {
        let data = cookie_bytes_36(24, 48_000, 2);
        let c = parse_alac_magic_cookie(&data).unwrap();
        assert_eq!(c.bit_depth, 24);
        assert_eq!(c.sample_rate, 48_000);
        assert_eq!(c.num_channels, 2);
    }
}

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use ape_decoder::ApeDecoder;
use flacenc::error::SourceError;
use flacenc::source::{Fill, Source};

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;

pub struct ApeSource {
    decoder: ApeDecoder<BufReader<File>>,
    channels: usize,
    bits_per_sample: usize,
    sample_rate: usize,
    bytes_per_sample: usize,
    total_samples: usize,
    current_frame: u32,
    pending: Vec<u8>,
}

impl ApeSource {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(ConvertError::Io)?;
        let decoder = ApeDecoder::new(BufReader::new(file))
            .map_err(|e| ConvertError::Decode(e.to_string()))?;
        let bits = decoder.info().bits_per_sample;
        if bits == 0 || bits > 32 {
            return Err(ConvertError::Decode(format!(
                "unsupported APE bit depth: {bits}"
            )));
        }
        let channels = decoder.info().channels as usize;
        let bytes_per_sample = (bits / 8) as usize;
        if channels == 0 || bytes_per_sample == 0 {
            return Err(ConvertError::Decode("invalid APE channel layout".into()));
        }
        let sample_rate = decoder.info().sample_rate as usize;
        let total_samples = decoder.info().total_samples as usize;
        Ok(Self {
            decoder,
            channels,
            bits_per_sample: bits as usize,
            sample_rate,
            bytes_per_sample,
            total_samples,
            current_frame: 0,
            pending: Vec::new(),
        })
    }

    fn load_next_frame(&mut self) -> std::result::Result<bool, SourceError> {
        if self.current_frame >= self.decoder.total_frames() {
            return Ok(false);
        }
        let pcm_bytes = self
            .decoder
            .decode_frame(self.current_frame)
            .map_err(|e| decode_err(e.to_string()))?;
        self.current_frame += 1;
        self.pending.extend_from_slice(&pcm_bytes);
        Ok(true)
    }
}

fn decode_err(msg: String) -> SourceError {
    SourceError::from_io_error(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        msg,
    ))
}

impl Source for ApeSource {
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
        Some(self.total_samples)
    }

    fn read_samples<F: Fill>(
        &mut self,
        block_size: usize,
        dest: &mut F,
    ) -> std::result::Result<usize, SourceError> {
        let frame_bytes = self.bytes_per_sample * self.channels;

        loop {
            let available = self.pending.len() / frame_bytes;
            if available >= block_size {
                break;
            }
            if !self.load_next_frame()? {
                break;
            }
        }

        let available = self.pending.len() / frame_bytes;
        if available == 0 {
            return Ok(0);
        }

        let frame_samples = block_size.min(available);
        let take_bytes = frame_samples * frame_bytes;
        dest.fill_le_bytes(&self.pending[..take_bytes], self.bytes_per_sample)?;
        self.pending.drain(..take_bytes);
        Ok(frame_samples)
    }
}

/// Full-buffer decode for tests and `decode_to_pcm`.
pub fn decode(path: &Path) -> Result<PcmBuffer> {
    let mut src = ApeSource::open(path)?;
    let mut fill = crate::source::collect::VecFill {
        samples: Vec::new(),
        channels: src.channels,
        bits_per_sample: src.bits_per_sample as u8,
    };
    let block = 4096usize;
    loop {
        let n = src
            .read_samples(block, &mut fill)
            .map_err(|e| ConvertError::Decode(e.to_string()))?;
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

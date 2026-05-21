use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use flacenc::error::SourceError;
use flacenc::source::{Fill, Source};
use hound::{SampleFormat, WavIntoSamples, WavReader};

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;
use crate::pcm_push::clamp_sample;

enum WavSampleIter {
    Int(WavIntoSamples<BufReader<File>, i32>),
    Float(WavIntoSamples<BufReader<File>, f32>),
}

impl WavSampleIter {
    fn next_sample(&mut self) -> Option<std::result::Result<i32, hound::Error>> {
        match self {
            Self::Int(it) => it.next(),
            Self::Float(it) => it.next().map(|r| r.map(|f| (f * i32::MAX as f32) as i32)),
        }
    }
}

pub struct WavSource {
    samples: WavSampleIter,
    channels: usize,
    bits_per_sample: usize,
    sample_rate: usize,
    total_samples: usize,
    samples_read: usize,
    bits: u8,
}

impl WavSource {
    pub fn open(path: &Path) -> Result<Self> {
        let reader = WavReader::open(path).map_err(|e| ConvertError::Decode(e.to_string()))?;
        let spec = reader.spec();
        let channels = spec.channels as usize;
        let bits_per_sample = spec.bits_per_sample as usize;
        let sample_rate = spec.sample_rate as usize;
        let total_samples = reader.duration() as usize;
        let bits = bits_per_sample as u8;

        let samples = match spec.sample_format {
            SampleFormat::Int => WavSampleIter::Int(reader.into_samples()),
            SampleFormat::Float => WavSampleIter::Float(reader.into_samples()),
        };

        Ok(Self {
            samples,
            channels,
            bits_per_sample,
            sample_rate,
            total_samples,
            samples_read: 0,
            bits,
        })
    }
}

fn decode_err(msg: String) -> SourceError {
    SourceError::from_io_error(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        msg,
    ))
}

impl Source for WavSource {
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
        let to_read = block_size.min(self.total_samples.saturating_sub(self.samples_read));
        if to_read == 0 {
            return Ok(0);
        }

        let mut chunk = Vec::with_capacity(to_read * self.channels);
        for _ in 0..to_read * self.channels {
            let s = self
                .samples
                .next_sample()
                .transpose()
                .map_err(|e| decode_err(e.to_string()))?
                .ok_or_else(|| decode_err("unexpected end of wav".into()))?;
            chunk.push(clamp_sample(s, self.bits));
        }

        dest.fill_interleaved(&chunk)?;
        self.samples_read += to_read;
        Ok(to_read)
    }
}

/// Full-buffer decode for tests and `decode_to_pcm`.
pub fn decode(path: &Path) -> Result<PcmBuffer> {
    let mut reader = WavReader::open(path).map_err(|e| ConvertError::Decode(e.to_string()))?;
    let spec = reader.spec();
    let channels = spec.channels as u8;
    let bits_per_sample = spec.bits_per_sample as u8;
    let sample_rate = spec.sample_rate;
    let bits = bits_per_sample;

    let samples: Vec<i32> = match spec.sample_format {
        SampleFormat::Int => reader
            .samples::<i32>()
            .map(|s| s.map(|v| clamp_sample(v, bits)).map_err(|e| ConvertError::Decode(e.to_string())))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| {
                s.map(|f| clamp_sample((f * i32::MAX as f32) as i32, bits))
                    .map_err(|e| ConvertError::Decode(e.to_string()))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };

    Ok(PcmBuffer {
        samples,
        channels,
        bits_per_sample,
        sample_rate,
    })
}

use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::Path;

use hound::{SampleFormat, WavIntoSamples, WavReader};

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;
use crate::pcm_push::clamp_sample;
use crate::source::traits::{Fill, PcmRead};

enum WavSampleIter {
    Int(WavIntoSamples<Box<dyn Read + Send>, i32>),
    Float(WavIntoSamples<Box<dyn Read + Send>, f32>),
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
        let file = File::open(path).map_err(ConvertError::Io)?;
        Self::from_reader(BufReader::new(file))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        Self::from_reader(Cursor::new(bytes))
    }

    pub fn from_reader<R>(reader: R) -> Result<Self>
    where
        R: Read + Send + 'static,
    {
        let reader = WavReader::new(Box::new(reader) as Box<dyn Read + Send>)
            .map_err(|e| ConvertError::Decode(e.to_string()))?;
        let spec = reader.spec();
        let channels = spec.channels as usize;
        let bits_per_sample = spec.bits_per_sample as usize;
        let sample_rate = spec.sample_rate as usize;
        let total_samples = reader.duration() as usize;
        let bits = bits_per_sample as u8;
        if spec.sample_format == SampleFormat::Float {
            return Err(ConvertError::Decode(
                "float WAV is not supported for browser-compatible FLAC; use integer PCM WAV"
                    .into(),
            ));
        }

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

fn decode_err(msg: String) -> ConvertError {
    ConvertError::Decode(msg)
}

impl PcmRead for WavSource {
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

    fn read_samples<F: Fill>(&mut self, block_size: usize, dest: &mut F) -> Result<usize> {
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
    if spec.sample_format == SampleFormat::Float {
        return Err(ConvertError::Decode(
            "float WAV is not supported for browser-compatible FLAC; use integer PCM WAV".into(),
        ));
    }

    let samples: Vec<i32> = match spec.sample_format {
        SampleFormat::Int => reader
            .samples::<i32>()
            .map(|s| {
                s.map(|v| clamp_sample(v, bits))
                    .map_err(|e| ConvertError::Decode(e.to_string()))
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    #[test]
    fn rejects_float_wav_instead_of_writing_32bit_flac() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("float.wav");
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let mut w = WavWriter::create(&path, spec).unwrap();
        w.write_sample(0.25f32).unwrap();
        w.write_sample(-0.25f32).unwrap();
        w.finalize().unwrap();

        let err = match WavSource::open(&path) {
            Ok(_) => panic!("float WAV should be rejected"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("float WAV"),
            "unexpected error: {err}"
        );
    }
}

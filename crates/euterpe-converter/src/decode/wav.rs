use std::path::Path;

use hound::{SampleFormat, WavReader};

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;

pub fn decode(path: &Path) -> Result<PcmBuffer> {
    let mut reader = WavReader::open(path).map_err(|e| ConvertError::Decode(e.to_string()))?;
    let spec = reader.spec();
    let channels = spec.channels as u8;
    let bits_per_sample = spec.bits_per_sample as u8;
    let sample_rate = spec.sample_rate;

    let samples: Vec<i32> = match spec.sample_format {
        SampleFormat::Int => reader
            .samples::<i32>()
            .map(|s| s.map_err(|e| ConvertError::Decode(e.to_string())))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| {
                s.map(|f| (f * i32::MAX as f32) as i32)
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

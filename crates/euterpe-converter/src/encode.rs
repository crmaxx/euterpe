pub mod libflac;

#[cfg(test)]
use crate::error::{ConvertError, Result};
#[cfg(test)]
use crate::pcm::PcmBuffer;

#[derive(Debug, Clone, Copy)]
pub struct EncodeProgress {
    pub flac_frames_encoded: u64,
    pub pcm_samples_read: u64,
    pub pcm_samples_total: Option<u64>,
}

/// Decode FLAC bytes to PCM for roundtrip verification (tests).
#[cfg(test)]
pub fn decode_flac_bytes(data: &[u8]) -> Result<PcmBuffer> {
    use std::io::Cursor;

    let cursor = Cursor::new(data);
    let mut reader =
        claxon::FlacReader::new(cursor).map_err(|e| ConvertError::Decode(e.to_string()))?;
    let streaminfo = reader.streaminfo();
    let channels = streaminfo.channels as u8;
    let bits_per_sample = streaminfo.bits_per_sample as u8;
    let sample_rate = streaminfo.sample_rate;

    let mut samples = Vec::new();
    for sample in reader.samples() {
        let s = sample.map_err(|e| ConvertError::Decode(e.to_string()))?;
        samples.push(s);
    }

    Ok(PcmBuffer {
        samples,
        channels,
        bits_per_sample,
        sample_rate,
    })
}

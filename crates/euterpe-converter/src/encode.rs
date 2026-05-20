use flacenc::bitsink::ByteSink;
use flacenc::component::BitRepr;
use flacenc::config::Encoder;
use flacenc::error::{Verify, Verified};
use flacenc::source::MemSource;
use flacenc::encode_with_fixed_block_size;

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;
use crate::settings::{FlacEncodeSettings, FlacPreset};

pub fn to_flacenc_encoder(settings: &FlacEncodeSettings) -> Result<Verified<Encoder>> {
    settings.validate()?;
    let mut enc = Encoder::default();
    enc.block_size = match settings.preset {
        FlacPreset::Fast => 4096,
        FlacPreset::Balanced => enc.block_size,
        FlacPreset::Best => 16_384,
    };
    if let Some(bs) = settings.block_size {
        enc.block_size = bs;
    }
    enc.multithread = settings.multithread;
    enc.into_verified()
        .map_err(|(_, e)| ConvertError::InvalidSettings(e.to_string()))
}

pub fn encode_flac(pcm: &PcmBuffer, settings: &FlacEncodeSettings) -> Result<Vec<u8>> {
    let config = to_flacenc_encoder(settings)?;
    let mut pcm = pcm.clone();
    pcm.clamp_to_bit_depth();

    let source = MemSource::from_samples(
        &pcm.samples,
        pcm.channels as usize,
        pcm.bits_per_sample as usize,
        pcm.sample_rate as usize,
    );

    let stream = encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| ConvertError::Encode(e.to_string()))?;

    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| ConvertError::Encode(e.to_string()))?;
    Ok(sink.as_slice().to_vec())
}

/// Decode FLAC bytes to PCM for roundtrip verification (tests).
#[cfg(test)]
pub fn decode_flac_bytes(data: &[u8]) -> Result<PcmBuffer> {
    use std::io::Cursor;

    let cursor = Cursor::new(data);
    let mut reader = claxon::FlacReader::new(cursor)
        .map_err(|e| ConvertError::Decode(e.to_string()))?;
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

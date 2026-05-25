use std::path::Path;

use flac_bound::{FlacEncoder, WriteWrapper};

use crate::encode::EncodeProgress;
use crate::error::{ConvertError, Result};
use crate::settings::{FlacEncodeSettings, FlacPreset};
use crate::source::collect::VecFill;
use crate::source::traits::{Fill, PcmRead};

struct InterleavedSliceSource<'a> {
    samples: &'a [i32],
    cursor: usize,
    channels: usize,
    bits_per_sample: usize,
    sample_rate: usize,
}

impl PcmRead for InterleavedSliceSource<'_> {
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
        Some(self.samples.len() / self.channels)
    }

    fn read_samples<F: Fill>(&mut self, block_size: usize, dest: &mut F) -> Result<usize> {
        if self.cursor >= self.samples.len() {
            return Ok(0);
        }
        let remaining_frames = (self.samples.len() - self.cursor) / self.channels;
        let frames = remaining_frames.min(block_size);
        let end = self.cursor + frames * self.channels;
        dest.fill_interleaved(&self.samples[self.cursor..end])?;
        self.cursor = end;
        Ok(frames)
    }
}

pub fn encode_flac_with_libflac<S>(
    mut src: S,
    dst: &Path,
    settings: &FlacEncodeSettings,
    mut on_progress: Option<&mut dyn FnMut(EncodeProgress)>,
) -> Result<()>
where
    S: PcmRead,
{
    settings.validate()?;
    let channels = src.channels() as u32;
    let bits_per_sample = src.bits_per_sample() as u32;
    let sample_rate = src.sample_rate() as u32;
    let total_samples = src.len_hint().map(|v| v as u64);
    let compression_level = match settings.preset {
        FlacPreset::Fast => 0,
        FlacPreset::Balanced => 5,
        FlacPreset::Best => 8,
    };
    let block_size = settings.block_size.unwrap_or(0) as u32;

    let config = FlacEncoder::new()
        .ok_or_else(|| ConvertError::Encode("libFLAC encoder allocation failed".into()))?
        .verify(true)
        .streamable_subset(true)
        .channels(channels)
        .bits_per_sample(bits_per_sample)
        .sample_rate(sample_rate)
        .compression_level(compression_level)
        .blocksize(block_size)
        .total_samples_estimate(total_samples.unwrap_or(0));

    let mut encoder = config
        .init_file(&dst)
        .map_err(|e| ConvertError::Encode(format!("libFLAC file init failed: {e:?}")))?;

    let mut frames_done = 0u64;
    let mut samples_done = 0u64;
    let chunk_frames = settings.block_size.unwrap_or(16_384);

    loop {
        let mut fill = VecFill {
            samples: Vec::new(),
            channels: src.channels(),
            bits_per_sample: src.bits_per_sample() as u8,
        };
        let read = src
            .read_samples(chunk_frames, &mut fill)
            .map_err(|e| ConvertError::Encode(e.to_string()))?;
        if read == 0 {
            break;
        }
        encoder
            .process_interleaved(&fill.samples, read as u32)
            .map_err(|()| {
                ConvertError::Encode(format!("libFLAC encode failed: {:?}", encoder.state()))
            })?;
        frames_done += 1;
        samples_done += read as u64;
        if let Some(cb) = on_progress.as_mut()
            && (frames_done == 1 || frames_done.is_multiple_of(32))
        {
            cb(EncodeProgress {
                flac_frames_encoded: frames_done,
                pcm_samples_read: samples_done,
                pcm_samples_total: total_samples,
            });
        }
    }

    encoder
        .finish()
        .map_err(|enc| ConvertError::Encode(format!("libFLAC finish failed: {:?}", enc.state())))?;
    Ok(())
}

pub fn encode_interleaved_pcm_to_flac(
    samples: &[i32],
    channels: usize,
    bits_per_sample: usize,
    sample_rate: usize,
    dst: &Path,
    settings: &FlacEncodeSettings,
    on_progress: Option<&mut dyn FnMut(EncodeProgress)>,
) -> Result<()> {
    if channels == 0 || !samples.len().is_multiple_of(channels) {
        return Err(ConvertError::Encode("PCM sample/channel mismatch".into()));
    }
    encode_flac_with_libflac(
        InterleavedSliceSource {
            samples,
            cursor: 0,
            channels,
            bits_per_sample,
            sample_rate,
        },
        dst,
        settings,
        on_progress,
    )
}

pub fn encode_interleaved_pcm_to_flac_bytes(
    samples: &[i32],
    channels: usize,
    bits_per_sample: usize,
    sample_rate: usize,
    settings: &FlacEncodeSettings,
    mut on_progress: Option<&mut dyn FnMut(EncodeProgress)>,
) -> Result<Vec<u8>> {
    if channels == 0 || !samples.len().is_multiple_of(channels) {
        return Err(ConvertError::Encode("PCM sample/channel mismatch".into()));
    }
    settings.validate()?;
    let total_samples = (samples.len() / channels) as u64;
    let compression_level = match settings.preset {
        FlacPreset::Fast => 0,
        FlacPreset::Balanced => 5,
        FlacPreset::Best => 8,
    };
    let block_size = settings.block_size.unwrap_or(0) as u32;

    let config = FlacEncoder::new()
        .ok_or_else(|| ConvertError::Encode("libFLAC encoder allocation failed".into()))?
        .verify(true)
        .streamable_subset(true)
        .channels(channels as u32)
        .bits_per_sample(bits_per_sample as u32)
        .sample_rate(sample_rate as u32)
        .compression_level(compression_level)
        .blocksize(block_size)
        .total_samples_estimate(total_samples);

    let mut out = Vec::new();
    {
        let mut wrapper = WriteWrapper(&mut out);
        let mut encoder = config
            .init_write(&mut wrapper)
            .map_err(|e| ConvertError::Encode(format!("libFLAC write init failed: {e:?}")))?;

        let mut frames_done = 0u64;
        let mut samples_done = 0u64;
        let chunk_frames = settings.block_size.unwrap_or(16_384);
        let mut cursor = 0usize;
        while cursor < samples.len() {
            let remaining_frames = (samples.len() - cursor) / channels;
            let read = remaining_frames.min(chunk_frames);
            let end = cursor + read * channels;
            encoder
                .process_interleaved(&samples[cursor..end], read as u32)
                .map_err(|()| {
                    ConvertError::Encode(format!("libFLAC encode failed: {:?}", encoder.state()))
                })?;
            cursor = end;
            frames_done += 1;
            samples_done += read as u64;
            if let Some(cb) = on_progress.as_mut()
                && (frames_done == 1 || frames_done.is_multiple_of(32))
            {
                cb(EncodeProgress {
                    flac_frames_encoded: frames_done,
                    pcm_samples_read: samples_done,
                    pcm_samples_total: Some(total_samples),
                });
            }
        }

        encoder.finish().map_err(|enc| {
            ConvertError::Encode(format!("libFLAC finish failed: {:?}", enc.state()))
        })?;
    }
    Ok(out)
}

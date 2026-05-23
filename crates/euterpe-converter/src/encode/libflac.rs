use std::path::Path;

use flac_bound::FlacEncoder;

use crate::encode::EncodeProgress;
use crate::error::{ConvertError, Result};
use crate::settings::{FlacEncodeSettings, FlacPreset};
use crate::source::collect::VecFill;
use crate::source::traits::PcmRead;

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

//! Stream PCM through FLAC encoder to a file (bounded decode memory).
//!
//! FLAC frames are collected via `flacenc` and written in one shot with `Stream::write`
//! (same bitstream layout as the reference encoder). PCM from ALAC/WAV/APE is accumulated
//! to full FLAC block sizes before each frame is encoded so no mid-stream short blocks
//! are emitted (required by strict decoders: Safari, libflac seek).

use std::io::Write;

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, Stream};
use flacenc::encode_fixed_size_frame;
use flacenc::error::SourceError;
use flacenc::source::{Context, Fill, FrameBuf, Source};

use crate::encode::to_flacenc_encoder;
use crate::error::{ConvertError, Result};
use crate::settings::FlacEncodeSettings;
use crate::source::collect::VecFill;

/// Callback during streaming encode (FLAC frames written).
#[derive(Debug, Clone, Copy)]
pub struct EncodeProgress {
    pub flac_frames_encoded: u64,
    pub pcm_samples_read: u64,
    pub pcm_samples_total: Option<u64>,
}

/// Encode `src` to `out`. PCM is read in blocks; compressed frames are buffered until
/// the stream is complete, then written as a valid FLAC file.
pub fn encode_flac_streaming<S, W>(
    mut src: S,
    settings: &FlacEncodeSettings,
    out: &mut W,
    on_progress: Option<&mut dyn FnMut(EncodeProgress)>,
) -> Result<()>
where
    S: Source,
    W: Write,
{
    if settings.multithread {
        tracing::warn!(
            "flac multithread is ignored for streaming encode; use converter parallelism for multiple files"
        );
    }

    let config = to_flacenc_encoder(settings)?;
    let block_size = config.block_size;
    let pcm_total = src.len_hint().map(|n| n as u64);
    let progress_interval = 32u64;

    let stream = encode_buffered_fixed_block(
        &config,
        &mut src,
        block_size,
        pcm_total,
        progress_interval,
        on_progress,
    )?;

    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| ConvertError::Encode(e.to_string()))?;
    out.write_all(sink.as_slice())
        .map_err(ConvertError::Io)?;
    Ok(())
}

/// Pull PCM from `src` until `pending` holds at least `block_size` frames, or input ends.
fn pull_pcm_to_block<S: Source>(
    src: &mut S,
    block_size: usize,
    channels: usize,
    bits_per_sample: u8,
    pending: &mut Vec<i32>,
) -> std::result::Result<bool, SourceError> {
    loop {
        let have = pending.len() / channels;
        if have >= block_size {
            return Ok(false);
        }
        let mut fill = VecFill {
            samples: Vec::new(),
            channels,
            bits_per_sample,
        };
        let read = src.read_samples(block_size, &mut fill)?;
        if read == 0 {
            return Ok(true);
        }
        pending.extend_from_slice(&fill.samples);
    }
}

fn encode_buffered_fixed_block<S: Source>(
    config: &flacenc::error::Verified<flacenc::config::Encoder>,
    src: &mut S,
    block_size: usize,
    pcm_total: Option<u64>,
    progress_interval: u64,
    mut on_progress: Option<&mut dyn FnMut(EncodeProgress)>,
) -> Result<Stream> {
    let channels = src.channels();
    let bits_per_sample = src.bits_per_sample();

    let mut stream = Stream::new(src.sample_rate(), channels, bits_per_sample)
        .map_err(|e| ConvertError::Encode(e.to_string()))?;
    stream
        .stream_info_mut()
        .set_block_sizes(block_size, block_size)
        .map_err(|e| ConvertError::Encode(e.to_string()))?;

    let mut framebuf = FrameBuf::with_size(channels, block_size)
        .map_err(|e| ConvertError::Encode(e.to_string()))?;
    let mut ctx = Context::new(bits_per_sample, channels);
    let mut pending: Vec<i32> = Vec::new();
    let mut flac_frames: u64 = 0;

    loop {
        let eof = pull_pcm_to_block(src, block_size, channels, bits_per_sample as u8, &mut pending)
            .map_err(|e| ConvertError::Encode(e.to_string()))?;
        let have = pending.len() / channels;
        if have == 0 {
            break;
        }

        let frame_samples = if eof {
            have
        } else {
            debug_assert!(have >= block_size);
            block_size
        };
        let take = frame_samples * channels;
        let chunk = &pending[..take];

        framebuf
            .fill_interleaved(chunk)
            .map_err(|e| ConvertError::Encode(e.to_string()))?;
        ctx.fill_interleaved(chunk)
            .map_err(|e| ConvertError::Encode(e.to_string()))?;

        // Fixed block size in STREAMINFO → frame-number headers (fixed blocking).
        // Do not switch to StartSample: that enables variable blocking and breaks Safari.
        let frame = encode_fixed_size_frame(
            config,
            &framebuf,
            ctx.current_frame_number()
                .expect("frame number set after fill"),
            stream.stream_info(),
        )
        .map_err(|e| ConvertError::Encode(e.to_string()))?;

        stream.add_frame(frame);
        pending.drain(..take);
        flac_frames += 1;

        if let Some(cb) = on_progress.as_mut()
            && (flac_frames == 1 || flac_frames.is_multiple_of(progress_interval))
        {
            cb(EncodeProgress {
                flac_frames_encoded: flac_frames,
                pcm_samples_read: ctx.total_samples() as u64,
                pcm_samples_total: pcm_total,
            });
        }

        if eof {
            break;
        }
    }

    stream
        .stream_info_mut()
        .set_md5_digest(&ctx.md5_digest());
    stream
        .stream_info_mut()
        .set_total_samples(ctx.total_samples());
    // flacenc may lower minimum blocksize to the final partial frame; libflac keeps
    // min=max=encoding block size (last frame may be shorter per spec).
    stream
        .stream_info_mut()
        .set_block_sizes(block_size, block_size)
        .map_err(|e| ConvertError::Encode(e.to_string()))?;

    Ok(stream)
}

#[cfg(test)]
mod tests {
    use flacenc::error::SourceError;
    use flacenc::source::{Fill, MemSource, Source};

    use super::*;
    use crate::encode::{decode_flac_bytes, encode_flac};
    use crate::pcm::PcmBuffer;

    fn streaminfo_block_sizes(flac: &[u8]) -> (u16, u16) {
        assert_eq!(&flac[0..4], b"fLaC");
        (
            u16::from_be_bytes([flac[8], flac[9]]),
            u16::from_be_bytes([flac[10], flac[11]]),
        )
    }

    /// Feeds PCM in small chunks to mimic Symphonia ALAC packet boundaries.
    struct ChunkedSource {
        inner: MemSource,
        chunk_frames: usize,
    }

    impl ChunkedSource {
        fn new(samples: &[i32], channels: usize, bits: usize, rate: usize, chunk_frames: usize) -> Self {
            Self {
                inner: MemSource::from_samples(samples, channels, bits, rate),
                chunk_frames,
            }
        }
    }

    impl Source for ChunkedSource {
        fn channels(&self) -> usize {
            self.inner.channels()
        }

        fn bits_per_sample(&self) -> usize {
            self.inner.bits_per_sample()
        }

        fn sample_rate(&self) -> usize {
            self.inner.sample_rate()
        }

        fn len_hint(&self) -> Option<usize> {
            self.inner.len_hint()
        }

        fn read_samples<F: Fill>(
            &mut self,
            _block_size: usize,
            dest: &mut F,
        ) -> std::result::Result<usize, SourceError> {
            self.inner
                .read_samples(self.chunk_frames, dest)
        }
    }

    #[test]
    fn streaming_pcm_matches_buffered_encode() {
        let samples: Vec<i32> = (0..20_000).map(|i| ((i * 13) % 30_000) as i32 - 15_000).collect();
        let settings = FlacEncodeSettings::default();

        let pcm = PcmBuffer {
            samples: samples.clone(),
            channels: 2,
            bits_per_sample: 24,
            sample_rate: 48_000,
        };
        let buffered = encode_flac(&pcm, &settings).unwrap();
        let buffered_pcm = decode_flac_bytes(&buffered).unwrap();

        let mut streaming = Vec::new();
        let source = MemSource::from_samples(&samples, 2, 24, 48_000);
        encode_flac_streaming(source, &settings, &mut streaming, None).unwrap();
        let streaming_pcm = decode_flac_bytes(&streaming).unwrap();

        assert_eq!(buffered_pcm.samples, streaming_pcm.samples);
        let (min, max) = streaminfo_block_sizes(&streaming);
        assert_eq!(min, max, "STREAMINFO must not advertise mid-stream variable block sizes");
    }

    #[test]
    fn buffered_encode_constant_blocksize_with_chunked_pcm_source() {
        let block_size = 4096usize;
        let channels = 2usize;
        let sample_count = block_size * 20 + 557;
        let samples: Vec<i32> = (0..sample_count * channels)
            .map(|i| (i % 500) as i32 - 250)
            .collect();
        let settings = FlacEncodeSettings {
            block_size: Some(block_size),
            ..FlacEncodeSettings::default()
        };

        let mut out = Vec::new();
        let src = ChunkedSource::new(&samples, channels, 24, 48_000, 1000);
        encode_flac_streaming(src, &settings, &mut out, None).unwrap();

        let (min, max) = streaminfo_block_sizes(&out);
        assert_eq!(
            min, block_size as u16,
            "STREAMINFO minimum blocksize must match encoder block (libflac/xrecode)"
        );
        assert_eq!(max, block_size as u16);

        let round = decode_flac_bytes(&out).unwrap();
        assert_eq!(round.samples.len(), sample_count * channels);
    }

    #[test]
    fn streaming_flac_roundtrip_24bit_pcm() {
        let samples: Vec<i32> = (0..8192)
            .map(|i| {
                let t = i as f64 / 48000.0;
                (f64::sin(t * 440.0 * std::f64::consts::TAU) * 4_000_000.0) as i32
            })
            .collect();
        let mut out = Vec::new();
        let source = MemSource::from_samples(&samples, 2, 24, 48_000);
        encode_flac_streaming(source, &FlacEncodeSettings::default(), &mut out, None).unwrap();
        let round = decode_flac_bytes(&out).unwrap();
        assert_eq!(round.bits_per_sample, 24);
        assert_eq!(round.sample_rate, 48_000);
        assert_eq!(round.samples.len(), samples.len());
        for (a, b) in round.samples.iter().zip(samples.iter()) {
            assert!((a - b).abs() <= 1, "sample mismatch: {a} vs {b}");
        }
    }

    #[test]
    fn streaming_flac_roundtrip_wav() {
        use hound::{SampleFormat, WavSpec, WavWriter};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let wav = dir.path().join("t.wav");
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(&wav, spec).unwrap();
        for i in 0..2048 {
            let s = (i % 200) as i16;
            w.write_sample(s).unwrap();
            w.write_sample(-s).unwrap();
        }
        w.finalize().unwrap();

        let mut src = crate::source::open_pcm_source(&wav).unwrap();
        let flac_path = dir.path().join("out.flac");
        let mut file = std::fs::File::create(&flac_path).unwrap();
        encode_flac_streaming(
            &mut src,
            &FlacEncodeSettings::default(),
            &mut file,
            None,
        )
        .unwrap();
        drop(file);

        let data = std::fs::read(&flac_path).unwrap();
        let round = decode_flac_bytes(&data).unwrap();
        let expected = crate::format::decode_to_pcm(&wav).unwrap();
        assert_eq!(round.samples, expected.samples);
    }
}

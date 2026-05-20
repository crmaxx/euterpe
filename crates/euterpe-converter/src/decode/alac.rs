use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_ALAC};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;

pub fn decode(path: &Path) -> Result<PcmBuffer> {
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

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec == CODEC_TYPE_ALAC)
        .ok_or(ConvertError::NotAlac)?;
    let track_id = track.id;

    let sample_rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2) as u8;
    let bits_per_sample = track
        .codec_params
        .bits_per_coded_sample
        .map(|b| b as u8)
        .unwrap_or(16);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| ConvertError::Decode(e.to_string()))?;

    let mut all_samples: Vec<i32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(Error::IoError(_)) => break,
            Err(e) => return Err(ConvertError::Decode(e.to_string())),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = decoder
            .decode(&packet)
            .map_err(|e| ConvertError::Decode(e.to_string()))?;

        append_buffer(decoded, &mut all_samples);
    }

    if all_samples.is_empty() {
        return Err(ConvertError::Decode("no audio decoded".into()));
    }

    Ok(PcmBuffer {
        samples: all_samples,
        channels,
        bits_per_sample,
        sample_rate,
    })
}

pub(crate) fn append_buffer(buf: AudioBufferRef<'_>, out: &mut Vec<i32>) {
    match buf {
        AudioBufferRef::F32(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    let s = b.chan(c)[f];
                    out.push((s * i32::MAX as f32) as i32);
                }
            }
        }
        AudioBufferRef::S16(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    out.push(i32::from(b.chan(c)[f]));
                }
            }
        }
        AudioBufferRef::S24(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    out.push(b.chan(c)[f].inner());
                }
            }
        }
        AudioBufferRef::S32(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    out.push(b.chan(c)[f]);
                }
            }
        }
        AudioBufferRef::U8(b) => {
            let ch = b.spec().channels.count();
            let frames = b.frames();
            for f in 0..frames {
                for c in 0..ch {
                    let u = b.chan(c)[f];
                    out.push((i32::from(u) - 128) << 8);
                }
            }
        }
        _ => {}
    }
}

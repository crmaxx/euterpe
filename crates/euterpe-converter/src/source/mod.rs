pub mod alac;
pub mod ape;
pub(crate) mod collect;
pub mod wav;

use std::path::Path;

use flacenc::error::SourceError;
use flacenc::source::Source;

use crate::error::{ConvertError, Result};
use crate::format::{detect_format, InputFormat};
use crate::pcm::PcmBuffer;

pub use alac::AlacSource;
pub use ape::ApeSource;
pub use wav::WavSource;

pub enum PcmSource {
    Wav(WavSource),
    Alac(AlacSource),
    Ape(Box<ApeSource>),
}

impl PcmSource {
    pub fn open(path: &Path) -> Result<Self> {
        let format = detect_format(path)
            .ok_or_else(|| ConvertError::UnsupportedFormat(path.display().to_string()))?;
        Ok(match format {
            InputFormat::Wav => Self::Wav(WavSource::open(path)?),
            InputFormat::Alac => Self::Alac(AlacSource::open(path)?),
            InputFormat::Ape => Self::Ape(Box::new(ApeSource::open(path)?)),
            #[cfg(feature = "wavpack")]
            InputFormat::WavPack => {
                return Err(ConvertError::UnsupportedFormat(
                    "WavPack not yet implemented".into(),
                ));
            }
        })
    }
}

impl Source for PcmSource {
    fn channels(&self) -> usize {
        match self {
            Self::Wav(s) => s.channels(),
            Self::Alac(s) => s.channels(),
            Self::Ape(s) => s.channels(),
        }
    }

    fn bits_per_sample(&self) -> usize {
        match self {
            Self::Wav(s) => s.bits_per_sample(),
            Self::Alac(s) => s.bits_per_sample(),
            Self::Ape(s) => s.bits_per_sample(),
        }
    }

    fn sample_rate(&self) -> usize {
        match self {
            Self::Wav(s) => s.sample_rate(),
            Self::Alac(s) => s.sample_rate(),
            Self::Ape(s) => s.sample_rate(),
        }
    }

    fn len_hint(&self) -> Option<usize> {
        match self {
            Self::Wav(s) => s.len_hint(),
            Self::Alac(s) => s.len_hint(),
            Self::Ape(s) => s.len_hint(),
        }
    }

    fn read_samples<F: flacenc::source::Fill>(
        &mut self,
        block_size: usize,
        dest: &mut F,
    ) -> std::result::Result<usize, SourceError> {
        match self {
            Self::Wav(s) => s.read_samples(block_size, dest),
            Self::Alac(s) => s.read_samples(block_size, dest),
            Self::Ape(s) => s.read_samples(block_size, dest),
        }
    }
}

/// Open a streaming PCM source for the file format.
pub fn open_pcm_source(path: &Path) -> Result<PcmSource> {
    PcmSource::open(path)
}

/// Full-buffer decode (tests and legacy callers).
pub fn decode_to_pcm(path: &Path) -> Result<PcmBuffer> {
    let format = detect_format(path)
        .ok_or_else(|| ConvertError::UnsupportedFormat(path.display().to_string()))?;
    match format {
        InputFormat::Wav => wav::decode(path),
        InputFormat::Alac => alac::decode(path),
        InputFormat::Ape => ape::decode(path),
        #[cfg(feature = "wavpack")]
        InputFormat::WavPack => Err(ConvertError::UnsupportedFormat(
            "WavPack not yet implemented".into(),
        )),
    }
}

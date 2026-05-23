use std::path::Path;

use crate::error::Result;
use crate::pcm::PcmBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Wav,
    Alac,
    Ape,
    #[cfg(feature = "wavpack")]
    WavPack,
}

pub fn detect_format(path: &Path) -> Option<InputFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "wav" | "wave" => Some(InputFormat::Wav),
        "m4a" | "mp4" | "caf" => Some(InputFormat::Alac),
        "ape" => Some(InputFormat::Ape),
        #[cfg(feature = "wavpack")]
        "wv" | "wavpack" => Some(InputFormat::WavPack),
        _ => None,
    }
}

pub fn is_convertible_extension(ext: &str) -> bool {
    match ext.to_ascii_lowercase().as_str() {
        "wav" | "wave" | "m4a" | "mp4" | "caf" | "ape" => true,
        #[cfg(feature = "wavpack")]
        "wv" | "wavpack" => true,
        _ => false,
    }
}

/// Full-buffer decode (tests and legacy callers). Production path uses [`crate::source::open_pcm_source`].
pub fn decode_to_pcm(path: &Path) -> Result<PcmBuffer> {
    crate::source::decode_to_pcm(path)
}

//! Lossless conversion to FLAC (WAV, ALAC in MP4/M4A, Monkey's Audio).
//!
//! WavPack (`wv`) is planned behind the `wavpack` feature flag.

mod convert;
mod decode;
mod encode;
mod error;
mod format;
mod pcm;
mod settings;
mod tags;

pub use convert::{convert_file, output_path_for, ConvertOptions, ConvertResult};
pub use encode::{encode_flac, to_flacenc_encoder};
pub use error::ConvertError;
pub use format::{decode_to_pcm, detect_format, is_convertible_extension, InputFormat};
pub use pcm::PcmBuffer;
pub use settings::{FilePolicy, FlacEncodeSettings, FlacPreset};

#[cfg(test)]
mod tests {
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    use super::*;

    fn write_test_wav(path: &std::path::Path, frames: usize) {
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            let s = (i as i32 * 97 % 10_000) as i16;
            w.write_sample(s).unwrap();
            w.write_sample(-s).unwrap();
        }
        w.finalize().unwrap();
    }

    fn pcm_equal(a: &PcmBuffer, b: &PcmBuffer) -> bool {
        a.channels == b.channels
            && a.sample_rate == b.sample_rate
            && a.bits_per_sample == b.bits_per_sample
            && a.samples == b.samples
    }

    #[test]
    fn wav_flac_pcm_roundtrip() {
        let dir = tempdir().unwrap();
        let wav = dir.path().join("test.wav");
        write_test_wav(&wav, 2048);

        let src_pcm = decode_to_pcm(&wav).unwrap();
        let settings = FlacEncodeSettings::default();
        let flac = encode_flac(&src_pcm, &settings).unwrap();
        let round = encode::decode_flac_bytes(&flac).unwrap();
        assert!(pcm_equal(&src_pcm, &round));
    }

    #[test]
    fn convert_file_wav_to_flac_sibling() {
        let dir = tempdir().unwrap();
        let wav = dir.path().join("track.wav");
        write_test_wav(&wav, 512);

        let result = convert_file(
            &wav,
            ConvertOptions {
                flac_encode: &FlacEncodeSettings::default(),
                file_policy: FilePolicy::SiblingThenDelete,
            },
        )
        .unwrap();

        assert!(result.output_path.ends_with("track.flac"));
        assert!(!wav.exists());
        assert!(result.output_path.exists());
    }

    #[test]
    fn flac_preset_fast_and_best_encode() {
        let dir = tempdir().unwrap();
        let wav = dir.path().join("t.wav");
        write_test_wav(&wav, 1024);
        let pcm = decode_to_pcm(&wav).unwrap();

        for preset in [FlacPreset::Fast, FlacPreset::Best] {
            let settings = FlacEncodeSettings {
                preset,
                ..Default::default()
            };
            let flac = encode_flac(&pcm, &settings).unwrap();
            let round = encode::decode_flac_bytes(&flac).unwrap();
            assert!(pcm_equal(&pcm, &round), "preset {:?}", preset);
        }
    }
}

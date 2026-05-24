//! Lossless conversion to FLAC (WAV, ALAC in MP4/M4A, Monkey's Audio).
//!
//! WavPack (`wv`) is available behind the `wavpack` feature flag.

mod convert;
mod decode;
mod encode;
mod error;
mod format;
mod pcm;
mod pcm_push;
mod settings;
mod source;
mod tags;

pub use convert::{ConvertOptions, ConvertProgress, ConvertResult, convert_file, output_path_for};
pub use encode::EncodeProgress;
pub use encode::libflac::{encode_flac_with_libflac, encode_interleaved_pcm_to_flac};
pub use error::ConvertError;
pub use format::{InputFormat, decode_to_pcm, detect_format, is_convertible_extension};
pub use pcm::PcmBuffer;
pub use settings::{FilePolicy, FlacEncodeSettings, FlacPreset};
pub use source::{PcmSource, open_pcm_source};
pub use tags::{ensure_libflac_metadata_tail, transfer_tags};

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

    fn write_i32_wav(path: &std::path::Path, frames: usize, sample_rate: u32) {
        let spec = WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            let s = ((i as i64 * 1_234_567) % i32::MAX as i64) as i32;
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

    fn flac_streaminfo_format(data: &[u8]) -> (u32, u8, u8) {
        assert_eq!(data.get(0..4), Some(b"fLaC".as_slice()));
        assert_eq!(data[4] & 0x7f, 0, "first metadata block must be STREAMINFO");
        let len = u32::from_be_bytes([0, data[5], data[6], data[7]]) as usize;
        assert_eq!(len, 34);
        let body = &data[8..42];
        let packed = u64::from_be_bytes([
            body[10], body[11], body[12], body[13], body[14], body[15], body[16], body[17],
        ]);
        let sample_rate = ((packed >> 44) & 0x000f_ffff) as u32;
        let channels = (((packed >> 41) & 0x7) + 1) as u8;
        let bits_per_sample = (((packed >> 36) & 0x1f) + 1) as u8;
        (sample_rate, channels, bits_per_sample)
    }

    #[test]
    fn wav_flac_pcm_roundtrip() {
        let dir = tempdir().unwrap();
        let wav = dir.path().join("test.wav");
        write_test_wav(&wav, 2048);

        let src_pcm = decode_to_pcm(&wav).unwrap();
        let result = convert_file(
            &wav,
            ConvertOptions {
                flac_encode: &FlacEncodeSettings::default(),
                file_policy: FilePolicy::SiblingThenDelete,
                on_progress: None,
            },
        )
        .unwrap();

        let data = std::fs::read(&result.output_path).unwrap();
        let round = encode::decode_flac_bytes(&data).unwrap();
        assert!(pcm_equal(&src_pcm, &round));
    }

    #[test]
    fn wav_32bit_192khz_preserves_stream_format() {
        let dir = tempdir().unwrap();
        let wav = dir.path().join("hires.wav");
        write_i32_wav(&wav, 1024, 192_000);

        let result = convert_file(
            &wav,
            ConvertOptions {
                flac_encode: &FlacEncodeSettings::default(),
                file_policy: FilePolicy::SiblingThenDelete,
                on_progress: None,
            },
        )
        .unwrap();

        let data = std::fs::read(&result.output_path).unwrap();
        assert_eq!(flac_streaminfo_format(&data), (192_000, 2, 32));
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
                on_progress: None,
            },
        )
        .unwrap();

        assert!(result.output_path.ends_with("track.flac"));
        assert!(!wav.exists());
        assert!(result.output_path.exists());
    }

    #[test]
    fn parallel_conversions_in_same_directory_do_not_share_temp_file() {
        let dir = tempdir().unwrap();
        let wav_a = dir.path().join("a.wav");
        let wav_b = dir.path().join("b.wav");
        write_test_wav(&wav_a, 65_536);
        write_test_wav(&wav_b, 65_536);
        let pcm_a = decode_to_pcm(&wav_a).unwrap();
        let pcm_b = decode_to_pcm(&wav_b).unwrap();
        let settings = FlacEncodeSettings::default();

        std::thread::scope(|scope| {
            let handle_a = scope.spawn(|| {
                convert_file(
                    &wav_a,
                    ConvertOptions {
                        flac_encode: &settings,
                        file_policy: FilePolicy::SiblingThenDelete,
                        on_progress: None,
                    },
                )
                .unwrap()
            });
            let handle_b = scope.spawn(|| {
                convert_file(
                    &wav_b,
                    ConvertOptions {
                        flac_encode: &settings,
                        file_policy: FilePolicy::SiblingThenDelete,
                        on_progress: None,
                    },
                )
                .unwrap()
            });
            let out_a = handle_a.join().unwrap().output_path;
            let out_b = handle_b.join().unwrap().output_path;
            let round_a = encode::decode_flac_bytes(&std::fs::read(out_a).unwrap()).unwrap();
            let round_b = encode::decode_flac_bytes(&std::fs::read(out_b).unwrap()).unwrap();
            assert!(pcm_equal(&pcm_a, &round_a));
            assert!(pcm_equal(&pcm_b, &round_b));
        });
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
            let out = dir.path().join(format!("{preset:?}.flac"));
            let src = open_pcm_source(&wav).unwrap();
            encode_flac_with_libflac(src, &out, &settings, None).unwrap();
            let round = encode::decode_flac_bytes(&std::fs::read(out).unwrap()).unwrap();
            assert!(pcm_equal(&pcm, &round), "preset {:?}", preset);
        }
    }
}

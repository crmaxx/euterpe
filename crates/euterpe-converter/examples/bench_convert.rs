//! Console benchmark for the streaming converter.
//!
//! Usage:
//! cargo run -p euterpe-converter --example bench_convert -- /path/to/input.m4a
//! cargo run -p euterpe-converter --release --example bench_convert -- /path/to/input.m4a

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use euterpe_converter::{
    ConvertError, FlacEncodeSettings, FlacPreset, encode_flac_with_libflac,
    ensure_libflac_metadata_tail, open_pcm_source, transfer_tags,
};

type Result<T> = std::result::Result<T, ConvertError>;

struct BenchCase {
    name: &'static str,
    settings: FlacEncodeSettings,
}

struct BenchResult {
    elapsed: Duration,
    output_bytes: u64,
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let input = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| usage_and_exit());
    let runs = args
        .next()
        .map(|value| value.to_string_lossy().parse::<usize>())
        .transpose()
        .map_err(|e| ConvertError::InvalidSettings(e.to_string()))?
        .unwrap_or(1);
    if runs == 0 {
        return Err(ConvertError::InvalidSettings(
            "runs must be greater than zero".into(),
        ));
    }

    let input_bytes = std::fs::metadata(&input).map_err(ConvertError::Io)?.len();
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    let cases = [
        BenchCase {
            name: "balanced-current",
            settings: FlacEncodeSettings::default(),
        },
        BenchCase {
            name: "fast",
            settings: FlacEncodeSettings {
                preset: FlacPreset::Fast,
                ..FlacEncodeSettings::default()
            },
        },
    ];

    println!(
        "profile={profile} input={} input_bytes={} runs={runs}",
        input.display(),
        input_bytes
    );
    println!("encoder=libFLAC");
    println!("case\tbest_s\tlast_s\tinput_mb_s\toutput_bytes");
    let keep_dir = std::env::var_os("EUTERPE_BENCH_KEEP_DIR").map(PathBuf::from);
    if let Some(dir) = keep_dir.as_deref() {
        std::fs::create_dir_all(dir).map_err(ConvertError::Io)?;
        println!("keep_dir={}", dir.display());
    }

    for case in cases {
        let mut best: Option<BenchResult> = None;
        let mut last: Option<BenchResult> = None;

        for _ in 0..runs {
            let keep_output = keep_dir
                .as_deref()
                .map(|dir| dir.join(format!("{}.flac", case.name)));
            let result = run_case(&input, &case.settings, keep_output.as_deref())?;
            if best
                .as_ref()
                .is_none_or(|current| result.elapsed < current.elapsed)
            {
                best = Some(BenchResult {
                    elapsed: result.elapsed,
                    output_bytes: result.output_bytes,
                });
            }
            last = Some(result);
        }

        let best = best.expect("at least one run");
        let last = last.expect("at least one run");
        let throughput = mib(input_bytes) / best.elapsed.as_secs_f64();
        println!(
            "{}\t{:.3}\t{:.3}\t{:.2}\t{}",
            case.name,
            best.elapsed.as_secs_f64(),
            last.elapsed.as_secs_f64(),
            throughput,
            best.output_bytes
        );
    }

    Ok(())
}

fn run_case(
    input: &Path,
    settings: &FlacEncodeSettings,
    keep_output: Option<&Path>,
) -> Result<BenchResult> {
    let dir = tempfile::tempdir().map_err(ConvertError::Io)?;
    let output = keep_output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dir.path().join("out.flac"));
    let pcm = open_pcm_source(input)?;

    let started = Instant::now();
    encode_flac_with_libflac(pcm, &output, settings, None)?;
    ensure_libflac_metadata_tail(&output)?;
    transfer_tags(input, &output)?;
    let elapsed = started.elapsed();
    let output_bytes = std::fs::metadata(output).map_err(ConvertError::Io)?.len();

    Ok(BenchResult {
        elapsed,
        output_bytes,
    })
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

fn usage_and_exit() -> ! {
    eprintln!("usage: bench_convert <input.{{wav,m4a,ape}}> [runs]");
    std::process::exit(2);
}

//! One-off: convert a file and run `flac -t` before/after tag transfer.
//!
//! Usage: cargo run -p euterpe-converter --example convert_check -- /path/to/file.m4a

use std::path::Path;
use std::process::Command;

use euterpe_converter::{convert_file, encode_flac_streaming, open_pcm_source, transfer_tags, ConvertOptions, FilePolicy, FlacEncodeSettings};

fn flac_test(path: &Path) -> String {
    let out = Command::new("flac")
        .args(["-t", path.to_str().unwrap()])
        .output();
    match out {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let status = if o.status.success() { "OK" } else { "FAIL" };
            format!("{status} exit={:?} stderr_len={}", o.status.code(), stderr.len())
        }
        Err(e) => format!("flac not run: {e}"),
    }
}

fn main() {
    let src = std::env::args()
        .nth(1)
        .expect("usage: convert_check <source.m4a>");
    let src = Path::new(&src);
    let settings = FlacEncodeSettings::default();

    let no_tags_out = src.with_extension("euterpe-notags.flac");
    println!("=== encode only (no tag transfer) ===");
    {
        let mut pcm = open_pcm_source(src).expect("open source");
        let mut f = std::fs::File::create(&no_tags_out).expect("create");
        encode_flac_streaming(&mut pcm, &settings, &mut f, None).expect("encode");
    }
    println!(
        "flac -t (no tags): {} ({} bytes)",
        flac_test(&no_tags_out),
        std::fs::metadata(&no_tags_out).map(|m| m.len()).unwrap_or(0)
    );

    println!("=== transfer_tags onto no-tags flac ===");
    let tagged_out = src.with_extension("euterpe-tagged.flac");
    std::fs::copy(&no_tags_out, &tagged_out).expect("copy");
    transfer_tags(src, &tagged_out).expect("transfer tags");
    println!(
        "flac -t (tags only): {} ({} bytes)",
        flac_test(&tagged_out),
        std::fs::metadata(&tagged_out).map(|m| m.len()).unwrap_or(0)
    );

    println!("=== full convert_file (encode + tags) ===");
    let work = tempfile::tempdir().expect("tempdir");
    let work_m4a = work.path().join(
        src.file_name()
            .expect("filename")
            .to_string_lossy()
            .as_ref(),
    );
    std::fs::copy(src, &work_m4a).expect("copy source");
    let result = convert_file(
        &work_m4a,
        ConvertOptions {
            flac_encode: &settings,
            file_policy: FilePolicy::SiblingThenDelete,
            on_progress: None,
        },
    )
    .expect("convert");
    println!("out: {}", result.output_path.display());
    println!(
        "flac -t (full): {} ({} bytes)",
        flac_test(&result.output_path),
        std::fs::metadata(&result.output_path)
            .map(|m| m.len())
            .unwrap_or(0)
    );
}

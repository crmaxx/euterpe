//! Apply tags from a source audio file onto an existing FLAC without re-encoding.
//!
//! Usage: apply_tags <tags-source.m4a> <target.flac>

use std::path::Path;
use std::process::Command;

use euterpe_converter::transfer_tags;

fn main() {
    let src_arg = std::env::args()
        .nth(1)
        .expect("usage: apply_tags <tags-source> <target.flac>");
    let dst_arg = std::env::args()
        .nth(2)
        .expect("usage: apply_tags <tags-source> <target.flac>");
    let src = Path::new(&src_arg);
    let dst = Path::new(&dst_arg);
    transfer_tags(src, dst).expect("transfer tags");
    let out = Command::new("flac")
        .args(["-t", dst.to_str().unwrap()])
        .output()
        .expect("flac");
    let status = if out.status.success() { "OK" } else { "FAIL" };
    eprintln!("flac -t: {status}");
    if !out.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
        std::process::exit(1);
    }
}

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::encode::encode_flac;
use crate::error::{ConvertError, Result};
use crate::format::decode_to_pcm;
use crate::settings::{FilePolicy, FlacEncodeSettings};
use crate::tags::transfer_tags;

#[derive(Debug, Clone)]
pub struct ConvertOptions<'a> {
    pub flac_encode: &'a FlacEncodeSettings,
    pub file_policy: FilePolicy,
}

#[derive(Debug, Clone)]
pub struct ConvertResult {
    pub output_path: PathBuf,
    pub bytes_written: u64,
}

pub fn output_path_for(src: &Path, policy: FilePolicy) -> PathBuf {
    match policy {
        FilePolicy::ReplaceInPlace => src.with_extension("flac"),
        FilePolicy::SiblingThenDelete => src.with_extension("flac"),
    }
}

pub fn convert_file(src: &Path, opts: ConvertOptions<'_>) -> Result<ConvertResult> {
    if !src.is_file() {
        return Err(ConvertError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("not a file: {}", src.display()),
        )));
    }

    let pcm = decode_to_pcm(src)?;
    let flac_data = encode_flac(&pcm, opts.flac_encode)?;
    let out_path = output_path_for(src, opts.file_policy);
    let parent = out_path
        .parent()
        .ok_or_else(|| ConvertError::Io(std::io::Error::other("no parent")))?;
    fs::create_dir_all(parent).map_err(ConvertError::Io)?;

    let temp_path = parent.join(format!(
        ".euterpe-convert-{}.flac.tmp",
        std::process::id()
    ));
    {
        let mut f = File::create(&temp_path).map_err(ConvertError::Io)?;
        f.write_all(&flac_data).map_err(ConvertError::Io)?;
        f.sync_all().map_err(ConvertError::Io)?;
    }

    fs::rename(&temp_path, &out_path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        ConvertError::Io(e)
    })?;

    transfer_tags(src, &out_path)?;

    if out_path != src {
        let _ = fs::remove_file(src);
    }

    let bytes_written = fs::metadata(&out_path)
        .map_err(ConvertError::Io)?
        .len();

    Ok(ConvertResult {
        output_path: out_path,
        bytes_written,
    })
}

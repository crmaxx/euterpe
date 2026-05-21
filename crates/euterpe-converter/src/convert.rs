use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::encode::streaming::{encode_flac_streaming, EncodeProgress};
use crate::error::{ConvertError, Result};
use crate::settings::{FilePolicy, FlacEncodeSettings};
use crate::source::open_pcm_source;
use crate::tags::{ensure_libflac_metadata_tail, transfer_tags};

#[derive(Debug, Clone, Copy)]
pub struct ConvertProgress {
    pub flac_frames_encoded: u64,
    pub pcm_samples_read: u64,
    pub pcm_samples_total: Option<u64>,
}

impl From<EncodeProgress> for ConvertProgress {
    fn from(p: EncodeProgress) -> Self {
        Self {
            flac_frames_encoded: p.flac_frames_encoded,
            pcm_samples_read: p.pcm_samples_read,
            pcm_samples_total: p.pcm_samples_total,
        }
    }
}

#[derive(Clone)]
pub struct ConvertOptions<'a> {
    pub flac_encode: &'a FlacEncodeSettings,
    pub file_policy: FilePolicy,
    pub on_progress: Option<Arc<dyn Fn(ConvertProgress) + Send + Sync>>,
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

    tracing::info!(path = %src.display(), "convert start");

    let mut pcm_src = open_pcm_source(src)?;
    let out_path = output_path_for(src, opts.file_policy);
    let parent = out_path
        .parent()
        .ok_or_else(|| ConvertError::Io(std::io::Error::other("no parent")))?;
    fs::create_dir_all(parent).map_err(ConvertError::Io)?;

    let temp_path = parent.join(format!(
        ".euterpe-convert-{}.flac.tmp",
        std::process::id()
    ));

    let encode_result = {
        let mut out = BufWriter::new(
            File::create(&temp_path).map_err(ConvertError::Io)?,
        );
        let mut progress_cb = |p: EncodeProgress| {
            if let Some(cb) = &opts.on_progress {
                cb(p.into());
            }
        };
        let result = encode_flac_streaming(
            &mut pcm_src,
            opts.flac_encode,
            &mut out,
            Some(&mut progress_cb),
        );
        if let Ok(inner) = out.into_inner() {
            let _ = inner.sync_all();
        }
        result
    };
    if let Err(e) = encode_result {
        let _ = fs::remove_file(&temp_path);
        return Err(e);
    }

    fs::rename(&temp_path, &out_path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        ConvertError::Io(e)
    })?;

    ensure_libflac_metadata_tail(&out_path)?;
    transfer_tags(src, &out_path)?;

    if out_path != src {
        let _ = fs::remove_file(src);
    }

    let bytes_written = fs::metadata(&out_path)
        .map_err(ConvertError::Io)?
        .len();

    tracing::info!(
        path = %src.display(),
        out = %out_path.display(),
        bytes = bytes_written,
        "convert done"
    );

    Ok(ConvertResult {
        output_path: out_path,
        bytes_written,
    })
}

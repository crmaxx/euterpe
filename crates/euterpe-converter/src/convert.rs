use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::encode::EncodeProgress;
use crate::encode::libflac::encode_flac_with_libflac;
use crate::error::{ConvertError, Result};
use crate::settings::{FilePolicy, FlacEncodeSettings};
use crate::source::open_pcm_source;
use crate::tags::{ensure_libflac_metadata_tail, transfer_tags};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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

fn create_temp_file(parent: &Path) -> Result<(PathBuf, File)> {
    loop {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!(
            ".euterpe-convert-{}-{n}.flac.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(ConvertError::Io(e)),
        }
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

    let pcm_src = open_pcm_source(src)?;
    let out_path = output_path_for(src, opts.file_policy);
    let parent = out_path
        .parent()
        .ok_or_else(|| ConvertError::Io(std::io::Error::other("no parent")))?;
    fs::create_dir_all(parent).map_err(ConvertError::Io)?;

    let (temp_path, temp_file) = create_temp_file(parent)?;

    drop(temp_file);

    let mut progress_cb = |p: EncodeProgress| {
        if let Some(cb) = &opts.on_progress {
            cb(p.into());
        }
    };
    let encode_result = encode_flac_with_libflac(
        pcm_src,
        &temp_path,
        opts.flac_encode,
        Some(&mut progress_cb),
    );
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

    let bytes_written = fs::metadata(&out_path).map_err(ConvertError::Io)?.len();

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

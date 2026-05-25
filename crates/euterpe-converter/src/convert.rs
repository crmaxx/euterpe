use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::encode::EncodeProgress;
use crate::encode::libflac::encode_interleaved_pcm_to_flac_bytes;
use crate::error::{ConvertError, Result};
use crate::settings::{FilePolicy, FlacEncodeSettings};
use crate::source::collect::VecFill;
use crate::source::open_pcm_source_bytes;
use crate::source::traits::PcmRead;
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

#[derive(Debug, Clone)]
pub struct ConvertInput {
    pub rel_path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ConvertOutput {
    pub rel_path: PathBuf,
    pub bytes: Vec<u8>,
    pub source_delete_rel: Option<PathBuf>,
}

pub fn output_path_for(src: &Path, policy: FilePolicy) -> PathBuf {
    match policy {
        FilePolicy::ReplaceInPlace => src.with_extension("flac"),
        FilePolicy::SiblingThenDelete => src.with_extension("flac"),
    }
}

pub fn convert_bytes(input: ConvertInput, opts: ConvertOptions<'_>) -> Result<ConvertOutput> {
    tracing::info!(path = %input.rel_path.display(), "convert bytes start");

    let out_path = output_path_for(&input.rel_path, opts.file_policy);
    let mut pcm_src = open_pcm_source_bytes(&input.rel_path, input.bytes)?;
    let channels = pcm_src.channels();
    let bits_per_sample = pcm_src.bits_per_sample();
    let sample_rate = pcm_src.sample_rate();

    let mut fill = VecFill {
        samples: Vec::new(),
        channels,
        bits_per_sample: bits_per_sample as u8,
    };
    let block_size = opts.flac_encode.block_size.unwrap_or(16_384);
    loop {
        let read = pcm_src.read_samples(block_size, &mut fill)?;
        if read == 0 {
            break;
        }
    }

    let mut progress_cb = |p: EncodeProgress| {
        if let Some(cb) = &opts.on_progress {
            cb(p.into());
        }
    };
    let bytes = encode_interleaved_pcm_to_flac_bytes(
        &fill.samples,
        channels,
        bits_per_sample,
        sample_rate,
        opts.flac_encode,
        Some(&mut progress_cb),
    )?;
    let source_delete_rel = if out_path != input.rel_path {
        Some(input.rel_path)
    } else {
        None
    };

    tracing::info!(
        path = %source_delete_rel.as_deref().unwrap_or(&out_path).display(),
        out = %out_path.display(),
        bytes = bytes.len(),
        "convert bytes done"
    );

    Ok(ConvertOutput {
        rel_path: out_path,
        bytes,
        source_delete_rel,
    })
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

    let input = ConvertInput {
        rel_path: src.to_path_buf(),
        bytes: fs::read(src).map_err(ConvertError::Io)?,
    };
    let output = convert_bytes(input, opts.clone())?;
    let out_path = output.rel_path;
    let parent = out_path
        .parent()
        .ok_or_else(|| ConvertError::Io(std::io::Error::other("no parent")))?;
    fs::create_dir_all(parent).map_err(ConvertError::Io)?;

    let (temp_path, mut temp_file) = create_temp_file(parent)?;
    if let Err(e) = temp_file.write_all(&output.bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(ConvertError::Io(e));
    }
    if let Err(e) = temp_file.sync_all() {
        let _ = fs::remove_file(&temp_path);
        return Err(ConvertError::Io(e));
    }
    drop(temp_file);

    fs::rename(&temp_path, &out_path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        ConvertError::Io(e)
    })?;

    ensure_libflac_metadata_tail(&out_path)?;
    transfer_tags(src, &out_path)?;

    if let Some(source_delete_rel) = output.source_delete_rel
        && source_delete_rel != out_path
    {
        let _ = fs::remove_file(source_delete_rel);
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

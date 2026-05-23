use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::ptr::NonNull;

use crate::error::{ConvertError, Result};
use crate::pcm::PcmBuffer;
use crate::source::collect::VecFill;
use crate::source::traits::{Fill, PcmRead};

pub struct WavPackSource {
    ctx: NonNull<wavpack_sys::WavpackContext>,
    channels: usize,
    bits_per_sample: usize,
    sample_rate: usize,
    total_samples: Option<usize>,
}

impl WavPackSource {
    pub fn open(path: &Path) -> Result<Self> {
        let path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| ConvertError::Decode("WavPack path contains NUL byte".into()))?;
        let mut error = [0 as c_char; 256];
        let flags = (wavpack_sys::OPEN_WVC
            | wavpack_sys::OPEN_FILE_UTF8
            | wavpack_sys::OPEN_ALT_TYPES) as i32;
        let raw = unsafe {
            wavpack_sys::WavpackOpenFileInput(path.as_ptr(), error.as_mut_ptr(), flags, 0)
        };
        let ctx = NonNull::new(raw).ok_or_else(|| {
            ConvertError::Decode(format!(
                "WavPack open failed: {}",
                unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
            ))
        })?;

        let mode = unsafe { wavpack_sys::WavpackGetMode(ctx.as_ptr()) };
        if mode & wavpack_sys::MODE_FLOAT as i32 != 0 {
            return Err(ConvertError::Decode(
                "floating-point WavPack is not supported for lossless FLAC conversion".into(),
            ));
        }
        if mode & wavpack_sys::MODE_LOSSLESS as i32 == 0 {
            return Err(ConvertError::Decode(
                "lossy WavPack is not supported for lossless FLAC conversion".into(),
            ));
        }

        let channels = unsafe { wavpack_sys::WavpackGetNumChannels(ctx.as_ptr()) };
        let bits_per_sample = unsafe { wavpack_sys::WavpackGetBitsPerSample(ctx.as_ptr()) };
        let sample_rate = unsafe { wavpack_sys::WavpackGetSampleRate(ctx.as_ptr()) };
        let total_samples = unsafe { wavpack_sys::WavpackGetNumSamples64(ctx.as_ptr()) };
        if channels <= 0 || bits_per_sample <= 0 || bits_per_sample > 32 || sample_rate == 0 {
            return Err(ConvertError::Decode("invalid WavPack stream format".into()));
        }

        Ok(Self {
            ctx,
            channels: channels as usize,
            bits_per_sample: bits_per_sample as usize,
            sample_rate: sample_rate as usize,
            total_samples: usize::try_from(total_samples).ok(),
        })
    }
}

impl Drop for WavPackSource {
    fn drop(&mut self) {
        unsafe {
            wavpack_sys::WavpackCloseFile(self.ctx.as_ptr());
        }
    }
}

impl PcmRead for WavPackSource {
    fn channels(&self) -> usize {
        self.channels
    }

    fn bits_per_sample(&self) -> usize {
        self.bits_per_sample
    }

    fn sample_rate(&self) -> usize {
        self.sample_rate
    }

    fn len_hint(&self) -> Option<usize> {
        self.total_samples
    }

    fn read_samples<F: Fill>(&mut self, block_size: usize, dest: &mut F) -> Result<usize> {
        let mut samples = vec![0i32; block_size * self.channels];
        let read = unsafe {
            wavpack_sys::WavpackUnpackSamples(
                self.ctx.as_ptr(),
                samples.as_mut_ptr(),
                block_size as u32,
            )
        } as usize;
        if read == 0 {
            return Ok(0);
        }
        samples.truncate(read * self.channels);
        dest.fill_interleaved(&samples)?;
        Ok(read)
    }
}

pub fn decode(path: &Path) -> Result<PcmBuffer> {
    let mut src = WavPackSource::open(path)?;
    let mut fill = VecFill {
        samples: Vec::new(),
        channels: src.channels,
        bits_per_sample: src.bits_per_sample as u8,
    };
    let block = 4096usize;
    loop {
        let n = src.read_samples(block, &mut fill)?;
        if n == 0 {
            break;
        }
    }
    if fill.samples.is_empty() {
        return Err(ConvertError::Decode("no audio decoded".into()));
    }
    Ok(PcmBuffer {
        samples: fill.samples,
        channels: src.channels as u8,
        bits_per_sample: src.bits_per_sample as u8,
        sample_rate: src.sample_rate as u32,
    })
}

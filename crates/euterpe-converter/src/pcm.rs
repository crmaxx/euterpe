/// Interleaved PCM samples (signed, per `bits_per_sample`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmBuffer {
    pub samples: Vec<i32>,
    pub channels: u8,
    pub bits_per_sample: u8,
    pub sample_rate: u32,
}

impl PcmBuffer {
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    pub fn max_abs_sample(&self) -> i32 {
        self.samples.iter().map(|s| s.abs()).max().unwrap_or(0)
    }

    /// Normalize samples to fit `bits_per_sample` (flacenc input range).
    pub fn clamp_to_bit_depth(&mut self) {
        let max = (1i32 << (self.bits_per_sample.saturating_sub(1))) - 1;
        let min = -max - 1;
        for s in &mut self.samples {
            *s = (*s).clamp(min, max);
        }
    }
}

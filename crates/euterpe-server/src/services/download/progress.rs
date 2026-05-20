use std::time::Instant;

/// Sliding estimate of bytes per second over ~1s windows.
pub struct SpeedMeter {
    last_at: Instant,
    last_bytes: u64,
    bps: u64,
}

impl Default for SpeedMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeedMeter {
    pub fn new() -> Self {
        Self {
            last_at: Instant::now(),
            last_bytes: 0,
            bps: 0,
        }
    }

    pub fn record_bytes(&mut self, total_bytes: u64) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_at).as_secs_f64();
        if elapsed >= 0.25 {
            let delta = total_bytes.saturating_sub(self.last_bytes);
            self.bps = (delta as f64 / elapsed) as u64;
            self.last_at = now;
            self.last_bytes = total_bytes;
        }
    }

    pub fn bps(&self) -> u64 {
        self.bps
    }
}

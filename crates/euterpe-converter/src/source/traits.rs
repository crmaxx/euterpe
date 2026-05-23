use crate::error::Result;

pub trait Fill {
    fn fill_interleaved(&mut self, interleaved: &[i32]) -> Result<()>;
    fn fill_le_bytes(&mut self, bytes: &[u8], bytes_per_sample: usize) -> Result<()>;
}

pub trait PcmRead {
    fn channels(&self) -> usize;
    fn bits_per_sample(&self) -> usize;
    fn sample_rate(&self) -> usize;
    fn len_hint(&self) -> Option<usize>;
    fn read_samples<F: Fill>(&mut self, block_size: usize, dest: &mut F) -> Result<usize>;
}

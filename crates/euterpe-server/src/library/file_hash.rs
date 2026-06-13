//! Content fingerprints for library files (xxHash64, lowercase 16-digit hex).

use xxhash_rust::xxh64::{Xxh64, xxh64};

/// Incremental xxHash64 (seed 0); digest matches [`content_hash_xxh64_chunks`].
pub struct ContentXxh64(Xxh64);

impl ContentXxh64 {
    pub fn new() -> Self {
        Self(Xxh64::new(0))
    }

    pub fn update(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.0.update(data);
        }
    }

    pub fn finish(self) -> String {
        format!("{:016x}", self.0.digest())
    }
}

impl Default for ContentXxh64 {
    fn default() -> Self {
        Self::new()
    }
}

/// xxHash64 over a single buffer (seed 0).
pub fn content_hash_xxh64(data: &[u8]) -> String {
    format!("{:016x}", xxh64(data, 0))
}

/// xxHash64 over multiple buffers in order (seed 0, empty chunks skipped).
pub fn content_hash_xxh64_chunks<'a, I>(chunks: I) -> String
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let mut hasher = ContentXxh64::new();
    for chunk in chunks {
        hasher.update(chunk);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_vector_is_known_digest() {
        assert_eq!(content_hash_xxh64(b""), "ef46db3751d8e999");
        assert_eq!(
            content_hash_xxh64_chunks([&[] as &[u8]]),
            "ef46db3751d8e999"
        );
    }

    #[test]
    fn chunks_match_single_buffer() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let whole = content_hash_xxh64(data);
        let split = content_hash_xxh64_chunks([&data[..19], &data[19..]]);
        assert_eq!(whole, split);
        let many = content_hash_xxh64_chunks([&data[..20], &data[20..]]);
        assert_eq!(whole, many);
    }
}

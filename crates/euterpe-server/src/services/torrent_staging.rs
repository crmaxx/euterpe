use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bytes::Bytes;

use crate::api::TorrentInspectFile;
use crate::error::ApiError;

const TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub struct StagingEntry {
    pub name: String,
    pub info_hash_v1: String,
    pub total_size_bytes: u64,
    pub comment: Option<String>,
    pub files: Vec<TorrentInspectFile>,
    pub magnet: Option<String>,
    pub torrent_bytes: Option<Bytes>,
    pub staging_dir: PathBuf,
    created_at: Instant,
}

impl StagingEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        info_hash_v1: String,
        total_size_bytes: u64,
        comment: Option<String>,
        files: Vec<TorrentInspectFile>,
        magnet: Option<String>,
        torrent_bytes: Option<Bytes>,
        staging_dir: PathBuf,
    ) -> Self {
        Self {
            name,
            info_hash_v1,
            total_size_bytes,
            comment,
            files,
            magnet,
            torrent_bytes,
            staging_dir,
            created_at: Instant::now(),
        }
    }
}

pub struct TorrentStaging {
    entries: Mutex<HashMap<String, StagingEntry>>,
}

impl Default for TorrentStaging {
    fn default() -> Self {
        Self::new()
    }
}

impl TorrentStaging {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert(&self, inspect_id: String, entry: StagingEntry) {
        let mut map = self.entries.lock().expect("staging lock");
        map.retain(|_, e| e.created_at.elapsed() < TTL);
        map.insert(inspect_id, entry);
    }

    pub fn get(&self, inspect_id: &str) -> Result<StagingEntry, ApiError> {
        let mut map = self.entries.lock().expect("staging lock");
        map.retain(|_, e| e.created_at.elapsed() < TTL);
        map.get(inspect_id)
            .cloned()
            .ok_or_else(|| ApiError::bad_request("inspect_id expired or unknown"))
    }

    pub fn remove(&self, inspect_id: &str) {
        let mut map = self.entries.lock().expect("staging lock");
        map.remove(inspect_id);
    }
}

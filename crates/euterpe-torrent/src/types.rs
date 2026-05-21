use std::num::NonZeroU32;
use std::path::PathBuf;

use bytes::Bytes;
use librqbit::limits::LimitsConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrqbitState {
    Initializing,
    Live,
    Paused,
    Error,
}

#[derive(Debug, Clone)]
pub struct InspectFile {
    pub index: usize,
    pub path: String,
    pub size_bytes: u64,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct InspectResult {
    pub name: String,
    pub info_hash_v1: String,
    pub total_size_bytes: u64,
    pub comment: Option<String>,
    pub files: Vec<InspectFile>,
    pub torrent_bytes: Option<Bytes>,
}

#[derive(Debug, Clone)]
pub struct StartJobRequest {
    pub magnet: Option<String>,
    pub torrent_bytes: Option<Bytes>,
    pub only_files: Vec<usize>,
    pub output_folder: PathBuf,
    pub ratelimits: LimitsConfig,
}

#[derive(Debug, Clone)]
pub struct JobHandle {
    pub librqbit_id: usize,
    pub info_hash: String,
}

#[derive(Debug, Clone)]
pub struct JobStats {
    pub progress_pct: f64,
    pub download_speed_bps: u64,
    pub upload_speed_bps: u64,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub finished: bool,
    pub librqbit_state: LibrqbitState,
    pub peers_live: u32,
    pub peers_connecting: u32,
    /// DHT routing table size (IPv4 + IPv6 nodes) for the shared librqbit session.
    pub dht_routing_nodes: u32,
    pub eta_secs: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct SessionSettings {
    pub disable_upload: bool,
    pub upload_bps: Option<NonZeroU32>,
    pub download_bps: Option<NonZeroU32>,
    /// Ask the router to forward the BitTorrent listen port via UPnP (librqbit-upnp).
    pub enable_upnp_port_forwarding: bool,
}

impl SessionSettings {
    pub fn limits_config(&self) -> LimitsConfig {
        LimitsConfig {
            upload_bps: self.upload_bps,
            download_bps: self.download_bps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InspectFile, JobStats, LibrqbitState};

    #[test]
    fn job_stats_carries_extended_fields() {
        let s = JobStats {
            progress_pct: 50.0,
            download_speed_bps: 1_000_000,
            upload_speed_bps: 100,
            progress_bytes: 500,
            total_bytes: 1000,
            finished: false,
            librqbit_state: LibrqbitState::Live,
            peers_live: 3,
            peers_connecting: 1,
            dht_routing_nodes: 42,
            eta_secs: Some(120),
            error: None,
        };
        assert_eq!(s.peers_live, 3);
        assert_eq!(s.librqbit_state, LibrqbitState::Live);
    }

    #[test]
    fn inspect_file_maps_index_and_path() {
        let f = InspectFile {
            index: 3,
            path: "music/track.flac".into(),
            size_bytes: 1024,
            selected: true,
        };
        assert_eq!(f.index, 3);
        assert_eq!(f.path, "music/track.flac");
        assert!(f.selected);
    }
}

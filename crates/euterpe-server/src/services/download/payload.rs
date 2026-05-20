use euterpe_torrent::{JobStats, LibrqbitState};
use serde::{Deserialize, Serialize};

use crate::api::{TorrentEuterpePhase, TorrentJobDetail, TorrentLibrqbitState};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DownloadJobPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_api_id: Option<String>,
    /// Human-readable label for queue UI (`Artist — Title`), not the Qobuz API ref.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torrent: Option<TorrentJobPayload>,
}

pub fn format_album_display_title(artist: &str, title: &str) -> String {
    let artist = artist.trim();
    let title = title.trim();
    if artist.is_empty() {
        title.to_string()
    } else if title.is_empty() {
        artist.to_string()
    } else {
        format!("{artist} — {title}")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TorrentRuntimeSnapshot {
    pub librqbit_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub euterpe_phase: Option<String>,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub download_speed_bps: u64,
    pub upload_speed_bps: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_secs: Option<u64>,
    pub peers_live: u32,
    pub peers_connecting: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TorrentRuntimeSnapshot {
    /// Shown while `start_job` is still resolving metadata / adding to session.
    pub fn connecting() -> Self {
        Self {
            librqbit_state: "initializing".into(),
            euterpe_phase: Some("downloading".into()),
            progress_bytes: 0,
            total_bytes: 0,
            download_speed_bps: 0,
            upload_speed_bps: 0,
            eta_secs: None,
            peers_live: 0,
            peers_connecting: 0,
            error: None,
        }
    }

    pub fn from_job_stats(stats: &JobStats, euterpe_phase: Option<TorrentEuterpePhase>) -> Self {
        Self {
            librqbit_state: match stats.librqbit_state {
                LibrqbitState::Initializing => "initializing".into(),
                LibrqbitState::Live => "live".into(),
                LibrqbitState::Paused => "paused".into(),
                LibrqbitState::Error => "error".into(),
            },
            euterpe_phase: euterpe_phase.map(|p| match p {
                TorrentEuterpePhase::Downloading => "downloading".into(),
                TorrentEuterpePhase::Importing => "importing".into(),
            }),
            progress_bytes: stats.progress_bytes,
            total_bytes: stats.total_bytes,
            download_speed_bps: stats.download_speed_bps,
            upload_speed_bps: stats.upload_speed_bps,
            eta_secs: stats.eta_secs,
            peers_live: stats.peers_live,
            peers_connecting: stats.peers_connecting,
            error: stats.error.clone(),
        }
    }

    pub fn to_api_detail(&self) -> TorrentJobDetail {
        let librqbit_state = match self.librqbit_state.as_str() {
            "paused" => TorrentLibrqbitState::Paused,
            "error" => TorrentLibrqbitState::Error,
            "live" => TorrentLibrqbitState::Live,
            _ => TorrentLibrqbitState::Initializing,
        };
        let euterpe_phase = self.euterpe_phase.as_deref().map(|p| match p {
            "importing" => TorrentEuterpePhase::Importing,
            _ => TorrentEuterpePhase::Downloading,
        });
        TorrentJobDetail {
            librqbit_state,
            euterpe_phase,
            progress_bytes: self.progress_bytes,
            total_bytes: self.total_bytes,
            download_speed_bps: self.download_speed_bps,
            upload_speed_bps: self.upload_speed_bps,
            eta_secs: self.eta_secs,
            peers_live: self.peers_live,
            peers_connecting: self.peers_connecting,
            error: self.error.clone(),
        }
    }
}

pub fn torrent_detail_from_stats(
    stats: &JobStats,
    euterpe_phase: Option<TorrentEuterpePhase>,
) -> TorrentJobDetail {
    TorrentRuntimeSnapshot::from_job_stats(stats, euterpe_phase).to_api_detail()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentJobPayload {
    pub display_name: String,
    pub info_hash: String,
    pub selected_file_indices: Vec<usize>,
    pub copy_to_library: bool,
    pub auto_index_after_import: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnet: Option<String>,
    pub save_dir_incoming: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_dest_rel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub librqbit_id: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<TorrentRuntimeSnapshot>,
}

impl DownloadJobPayload {
    pub fn display_title(&self, job_type: crate::api::DownloadJobType) -> String {
        match job_type {
            crate::api::DownloadJobType::Torrent => self
                .torrent
                .as_ref()
                .map(|t| t.display_name.clone())
                .unwrap_or_else(|| "Torrent".into()),
            _ => self
                .display_title
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Album download".into()),
        }
    }

    pub fn source(&self, job_type: crate::api::DownloadJobType) -> crate::api::DownloadSource {
        match job_type {
            crate::api::DownloadJobType::Torrent => crate::api::DownloadSource::Torrent,
            _ => crate::api::DownloadSource::Qobuz,
        }
    }

    pub fn torrent_detail_for_api(&self) -> Option<TorrentJobDetail> {
        self.torrent
            .as_ref()?
            .runtime
            .as_ref()
            .map(TorrentRuntimeSnapshot::to_api_detail)
    }

    pub fn set_torrent_runtime(&mut self, snapshot: TorrentRuntimeSnapshot) {
        if let Some(t) = &mut self.torrent {
            t.runtime = Some(snapshot);
        }
    }
}

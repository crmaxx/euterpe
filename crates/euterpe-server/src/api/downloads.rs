use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadJobType {
    Album,
    Track,
    Artist,
    Playlist,
    Torrent,
}

impl DownloadJobType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Album => "album",
            Self::Track => "track",
            Self::Artist => "artist",
            Self::Playlist => "playlist",
            Self::Torrent => "torrent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadSource {
    Qobuz,
    Torrent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDownloadRequest {
    pub job_type: DownloadJobType,
    /// Qobuz `album/get` id (short ref e.g. `zg7pv28g4mldg` or long slug). See `album_api_id` on favorites.
    pub album_api_id: String,
    pub quality: u8,
    /// Catalog `qobuz_id` for dedup and display; optional if unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qobuz_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDownloadByUrlRequest {
    pub url: String,
    pub quality: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDownloadResponse {
    pub job_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TorrentLibrqbitState {
    Initializing,
    Live,
    Paused,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TorrentEuterpePhase {
    Downloading,
    Importing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentJobDetail {
    pub librqbit_state: TorrentLibrqbitState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub euterpe_phase: Option<TorrentEuterpePhase>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadJob {
    pub id: i64,
    pub status: DownloadJobStatus,
    pub job_type: DownloadJobType,
    pub source: DownloadSource,
    pub display_title: String,
    pub qobuz_id: i64,
    pub quality: i32,
    pub progress_pct: f64,
    pub download_speed_bps: u64,
    pub queue_position: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torrent_detail: Option<TorrentJobDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchDownloadPriorityRequest {
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadJobListResponse {
    pub items: Vec<DownloadJob>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPurgeResponse {
    pub deleted: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgressEvent {
    pub id: i64,
    pub progress_pct: f64,
    pub download_speed_bps: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torrent_detail: Option<TorrentJobDetail>,
}

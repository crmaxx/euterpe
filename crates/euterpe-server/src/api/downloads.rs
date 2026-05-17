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
}

impl DownloadJobType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Album => "album",
            Self::Track => "track",
            Self::Artist => "artist",
            Self::Playlist => "playlist",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDownloadRequest {
    pub job_type: DownloadJobType,
    pub qobuz_id: u64,
    pub quality: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDownloadResponse {
    pub job_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadJob {
    pub id: i64,
    pub status: DownloadJobStatus,
    pub job_type: DownloadJobType,
    pub qobuz_id: i64,
    pub quality: i32,
    pub progress_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadJobListResponse {
    pub items: Vec<DownloadJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgressEvent {
    pub id: i64,
    pub progress_pct: f64,
}

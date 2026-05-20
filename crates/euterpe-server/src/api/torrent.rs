use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInspectFile {
    pub index: usize,
    pub path: String,
    pub size_bytes: u64,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInspectResponse {
    pub inspect_id: String,
    pub name: String,
    pub total_size_bytes: u64,
    pub info_hash_v1: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info_hash_v2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_space_bytes: Option<u64>,
    pub files: Vec<TorrentInspectFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TorrentInspectMagnetRequest {
    pub magnet: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TorrentConfirmFile {
    pub index: usize,
    pub selected: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TorrentConfirmRequest {
    pub inspect_id: String,
    pub files: Vec<TorrentConfirmFile>,
    pub copy_to_library: bool,
    pub auto_index_after_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSettings {
    pub seed_ratio_limit: f64,
    pub seed_time_limit_sec: u64,
    pub max_upload_kib_per_sec: u64,
}

impl Default for TorrentSettings {
    fn default() -> Self {
        Self {
            seed_ratio_limit: 0.0,
            seed_time_limit_sec: 0,
            max_upload_kib_per_sec: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSettingsResponse {
    #[serde(flatten)]
    pub settings: TorrentSettings,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TorrentSettingsPatch {
    #[serde(default)]
    pub seed_ratio_limit: Option<f64>,
    #[serde(default)]
    pub seed_time_limit_sec: Option<u64>,
    #[serde(default)]
    pub max_upload_kib_per_sec: Option<u64>,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_download_capability: Option<TorrentPostDownloadCapability>,
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
    #[serde(default)]
    pub post_download: Option<TorrentPostDownloadOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentCueCandidate {
    pub cue_path: String,
    pub audio_path: String,
    pub audio_format: String,
    pub direct_split_supported: bool,
    pub convert_required_for_split: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentPostDownloadCapability {
    pub cue_candidates: Vec<TorrentCueCandidate>,
    pub has_flac_image_cue: bool,
    pub has_convertible_image_cue: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TorrentPostDownloadOptions {
    #[serde(default)]
    pub convert_after_download: bool,
    #[serde(default)]
    pub split_after_download: bool,
    #[serde(default)]
    pub split_after_conversion: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file_policy: Option<String>,
}

/// Persisted torrent session options (maps to librqbit `disable_upload` + ratelimits).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSettings {
    /// When true, librqbit does not upload to peers (download-only).
    #[serde(default = "default_disable_upload")]
    pub disable_upload: bool,
    /// Upload cap in KiB/s; 0 = no cap. Ignored when `disable_upload` is true.
    pub max_upload_kib_per_sec: u64,
}

fn default_disable_upload() -> bool {
    true
}

impl Default for TorrentSettings {
    fn default() -> Self {
        Self {
            disable_upload: true,
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
    pub disable_upload: Option<bool>,
    #[serde(default)]
    pub max_upload_kib_per_sec: Option<u64>,
}

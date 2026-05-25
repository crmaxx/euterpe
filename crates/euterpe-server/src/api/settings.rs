use serde::{Deserialize, Serialize};

pub use crate::services::app_settings::{
    ConverterSettings, DownloadsSettings, FilePolicyDto, FlacEncodeSettingsDto, FlacPresetDto,
    LibraryScanSettings, StorageLocation, StorageSettings, UiLocale, UiPreferences, UiTheme,
};
use crate::services::storage_watch::{StorageWatchState, StorageWatchStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferencesResponse {
    pub settings: UiPreferences,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiPreferencesPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<UiTheme>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<UiLocale>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_quality: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConverterSettingsResponse {
    pub settings: ConverterSettings,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConverterSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_policy: Option<FilePolicyDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelism: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formats: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flac_encode: Option<FlacEncodeSettingsPatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FlacEncodeSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<FlacPresetDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size: Option<Option<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multithread: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryScanSettingsResponse {
    pub settings: LibraryScanSettings,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LibraryScanSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_workers: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_workers: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_queue_capacity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_queue_capacity: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadsSettingsResponse {
    pub settings: DownloadsSettings,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DownloadsSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettingsResponse {
    pub settings: StorageSettingsView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettingsView {
    pub library: Option<StorageLocationView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageLocationView {
    Local {
        path: String,
        watch_status: StorageWatchStatusView,
    },
    Smb {
        host: String,
        port: u16,
        share: String,
        path: String,
        watch_status: StorageWatchStatusView,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        workgroup: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageWatchStatusView {
    pub state: StorageWatchStateView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageWatchStateView {
    Disabled,
    Connected,
    Degraded,
    Reconnecting,
}

impl From<StorageWatchStatus> for StorageWatchStatusView {
    fn from(value: StorageWatchStatus) -> Self {
        Self {
            state: match value.state {
                StorageWatchState::Disabled => StorageWatchStateView::Disabled,
                StorageWatchState::Connected => StorageWatchStateView::Connected,
                StorageWatchState::Degraded => StorageWatchStateView::Degraded,
                StorageWatchState::Reconnecting => StorageWatchStateView::Reconnecting,
            },
            degraded_reason: value.degraded_reason,
        }
    }
}

impl From<&StorageSettings> for StorageSettingsView {
    fn from(value: &StorageSettings) -> Self {
        Self::from_with_watch_status(value, StorageWatchStatus::disabled())
    }
}

impl StorageSettingsView {
    pub fn from_with_watch_status(
        value: &StorageSettings,
        watch_status: StorageWatchStatus,
    ) -> Self {
        let watch_status = StorageWatchStatusView::from(watch_status);
        let library = value.library.as_ref().map(|library| match library {
            StorageLocation::Local { path } => StorageLocationView::Local {
                path: path.clone(),
                watch_status: StorageWatchStatusView::from(StorageWatchStatus::disabled()),
            },
            StorageLocation::Smb {
                host,
                port,
                share,
                path,
                username,
                workgroup,
                ..
            } => StorageLocationView::Smb {
                host: host.clone(),
                port: *port,
                share: share.clone(),
                path: path.clone(),
                watch_status,
                username: username.clone(),
                workgroup: workgroup.clone(),
            },
        });
        Self { library }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageSettingsPatch {
    pub library: StorageLocationPatch,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageLocationPatch {
    Local {
        path: String,
    },
    Smb {
        host: String,
        #[serde(default = "default_smb_port")]
        port: u16,
        share: String,
        #[serde(default)]
        path: String,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
        #[serde(default)]
        workgroup: Option<String>,
    },
}

fn default_smb_port() -> u16 {
    445
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageTestRequest {
    pub location: StorageLocationPatch,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageTestResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageBrowseResponse {
    pub entries: Vec<StorageBrowseEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageBrowseEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmbSharesRequest {
    pub host: String,
    #[serde(default = "default_smb_port")]
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub workgroup: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmbSharesResponse {
    pub shares: Vec<String>,
}

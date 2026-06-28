use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

use crate::api::QobuzSyncRunSummary;
pub use crate::services::app_settings::{
    ConverterSettings, DownloadsSettings, FilePolicyDto, FlacEncodeSettingsDto, FlacPresetDto,
    LibraryScanSettings, QobuzScheduledSyncSettings, StorageLocation, StorageSettings, UiLocale,
    UiPreferences, UiTheme,
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
pub struct QobuzScheduledSyncSettingsResponse {
    pub settings: QobuzScheduledSyncSettings,
    pub status: QobuzScheduledSyncStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzScheduledSyncStatus {
    pub server_timezone: String,
    pub next_run_at: Option<String>,
    pub last_run: Option<QobuzSyncRunSummary>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QobuzScheduledSyncSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron_expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_download_new_favorites: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettingsResponse {
    pub settings: StorageSettingsView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_migration_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommend_full_scan: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettingsView {
    pub library: Option<StorageLocationView>,
    pub presets: Vec<StoragePresetView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePresetView {
    pub id: String,
    pub label: String,
    pub kind: String,
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
        password_configured: bool,
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
        let library = value
            .library
            .as_ref()
            .map(|library| storage_location_view(library, watch_status));
        let presets = value
            .presets
            .iter()
            .map(|preset| StoragePresetView {
                id: preset.id.clone(),
                label: preset.label.clone(),
                kind: match &preset.location {
                    StorageLocation::Local { .. } => "local".to_string(),
                    StorageLocation::Smb { .. } => "smb".to_string(),
                },
            })
            .collect();
        Self { library, presets }
    }
}

fn storage_location_view(
    library: &StorageLocation,
    watch_status: StorageWatchStatusView,
) -> StorageLocationView {
    match library {
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
            password_encrypted,
        } => StorageLocationView::Smb {
            host: host.clone(),
            port: *port,
            share: share.clone(),
            path: path.clone(),
            watch_status,
            username: username.clone(),
            workgroup: workgroup.clone(),
            password_configured: password_encrypted.is_some(),
        },
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageSettingsPatch {
    #[serde(default)]
    pub activate_preset_id: Option<String>,
    #[serde(default)]
    pub library: Option<StorageLocationPatch>,
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
        username: StringPatchField,
        #[serde(default)]
        password: StringPatchField,
        #[serde(default)]
        workgroup: StringPatchField,
    },
}

#[derive(Debug, Clone, Default)]
pub enum StringPatchField {
    #[default]
    Missing,
    Clear,
    Value(String),
}

impl<'de> Deserialize<'de> for StringPatchField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match Option::<String>::deserialize(deserializer)? {
            Some(value) => Self::Value(value),
            None => Self::Clear,
        })
    }
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

#[derive(Debug, Clone, Deserialize)]
pub struct StorageBrowseRequest {
    pub location: StorageLocationPatch,
    #[serde(default)]
    pub path: Option<String>,
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

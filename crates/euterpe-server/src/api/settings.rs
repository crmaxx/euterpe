use serde::{Deserialize, Serialize};

pub use crate::services::app_settings::{
    ConverterSettings, DownloadsSettings, FilePolicyDto, FlacEncodeSettingsDto, FlacPresetDto,
    LibraryScanSettings, UiLocale, UiPreferences, UiTheme,
};

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

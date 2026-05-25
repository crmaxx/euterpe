use std::sync::Arc;

use euterpe_converter::{FilePolicy, FlacEncodeSettings, FlacPreset};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::config::{AppConfig, LibraryScanConfig};
use crate::db::settings;
use crate::error::ApiError;

pub const KEY_UI_PREFERENCES: &str = "ui.preferences";
pub const KEY_CONVERTER_SETTINGS: &str = "converter.settings";
pub const KEY_LIBRARY_SCAN_SETTINGS: &str = "library.scan.settings";
pub const KEY_DOWNLOADS_SETTINGS: &str = "downloads.settings";
pub const KEY_STORAGE_SETTINGS: &str = "storage.settings";

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiTheme {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiLocale {
    #[default]
    En,
    Ru,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiPreferences {
    #[serde(default)]
    pub theme: UiTheme,
    #[serde(default)]
    pub locale: UiLocale,
    #[serde(default = "default_quality")]
    pub default_quality: u8,
}

fn default_quality() -> u8 {
    6
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: UiTheme::default(),
            locale: UiLocale::default(),
            default_quality: default_quality(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlacEncodeSettingsDto {
    #[serde(default)]
    pub preset: FlacPresetDto,
    #[serde(default)]
    pub block_size: Option<usize>,
    #[serde(default)]
    pub multithread: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FlacPresetDto {
    #[default]
    Fast,
    Balanced,
    Best,
}

impl From<FlacPresetDto> for FlacPreset {
    fn from(v: FlacPresetDto) -> Self {
        match v {
            FlacPresetDto::Fast => FlacPreset::Fast,
            FlacPresetDto::Balanced => FlacPreset::Balanced,
            FlacPresetDto::Best => FlacPreset::Best,
        }
    }
}

impl Default for FlacEncodeSettingsDto {
    fn default() -> Self {
        Self {
            preset: FlacPresetDto::Balanced,
            block_size: None,
            multithread: false,
        }
    }
}

impl From<&FlacEncodeSettingsDto> for FlacEncodeSettings {
    fn from(d: &FlacEncodeSettingsDto) -> Self {
        FlacEncodeSettings {
            preset: d.preset.into(),
            block_size: d.block_size,
            multithread: d.multithread,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilePolicyDto {
    ReplaceInPlace,
    #[default]
    SiblingThenDelete,
}

impl From<FilePolicyDto> for FilePolicy {
    fn from(v: FilePolicyDto) -> Self {
        match v {
            FilePolicyDto::ReplaceInPlace => FilePolicy::ReplaceInPlace,
            FilePolicyDto::SiblingThenDelete => FilePolicy::SiblingThenDelete,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConverterSettings {
    #[serde(default)]
    pub auto_enabled: bool,
    #[serde(default)]
    pub file_policy: FilePolicyDto,
    #[serde(default = "default_parallelism")]
    pub parallelism: u32,
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,
    #[serde(default)]
    pub flac_encode: FlacEncodeSettingsDto,
}

fn default_parallelism() -> u32 {
    5
}

fn default_formats() -> Vec<String> {
    vec!["wav".into(), "m4a".into(), "ape".into()]
}

impl Default for ConverterSettings {
    fn default() -> Self {
        Self {
            auto_enabled: false,
            file_policy: FilePolicyDto::SiblingThenDelete,
            parallelism: 5,
            formats: default_formats(),
            flac_encode: FlacEncodeSettingsDto::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibraryScanSettings {
    #[serde(default = "default_worker_total")]
    pub worker_total: u32,
    #[serde(default = "default_enum_workers")]
    pub enum_workers: u32,
    #[serde(default = "default_process_workers")]
    pub process_workers: u32,
    #[serde(default)]
    pub seed_depth: u32,
    #[serde(default = "default_index_queue")]
    pub index_queue_capacity: u32,
    #[serde(default = "default_path_queue")]
    pub path_queue_capacity: u32,
}

fn default_worker_total() -> u32 {
    10
}
fn default_enum_workers() -> u32 {
    5
}
fn default_process_workers() -> u32 {
    5
}
fn default_index_queue() -> u32 {
    512
}
fn default_path_queue() -> u32 {
    2048
}

impl Default for LibraryScanSettings {
    fn default() -> Self {
        Self {
            worker_total: 10,
            enum_workers: 5,
            process_workers: 5,
            seed_depth: 1,
            index_queue_capacity: 512,
            path_queue_capacity: 2048,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadsSettings {
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
}

fn default_concurrency() -> u32 {
    3
}

impl Default for DownloadsSettings {
    fn default() -> Self {
        Self { concurrency: 3 }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeSettings {
    pub ui: UiPreferences,
    pub converter: ConverterSettings,
    pub library_scan: LibraryScanSettings,
    pub downloads: DownloadsSettings,
    pub storage: StorageSettings,
}

impl RuntimeSettings {
    pub fn library_scan_config(&self, debug: bool) -> Result<LibraryScanConfig, ApiError> {
        self.library_scan.to_config(debug)
    }
}

impl LibraryScanSettings {
    pub fn from_config(c: &LibraryScanConfig) -> Self {
        Self {
            worker_total: c.worker_total as u32,
            enum_workers: c.enum_workers as u32,
            process_workers: c.process_workers as u32,
            seed_depth: c.seed_depth,
            index_queue_capacity: c.index_queue_capacity as u32,
            path_queue_capacity: c.path_queue_capacity as u32,
        }
    }

    pub fn to_config(&self, debug: bool) -> Result<LibraryScanConfig, ApiError> {
        let worker_total = self.worker_total.clamp(2, 32) as usize;
        let enum_workers = self.enum_workers as usize;
        let process_workers = self.process_workers as usize;
        if enum_workers < 1 || process_workers < 1 {
            return Err(ApiError::bad_request(
                "enum_workers and process_workers must be >= 1",
            ));
        }
        if enum_workers > worker_total || process_workers > worker_total {
            return Err(ApiError::bad_request(
                "enum_workers and process_workers must be <= worker_total",
            ));
        }
        if enum_workers + process_workers > worker_total {
            return Err(ApiError::bad_request(
                "enum_workers + process_workers must be <= worker_total",
            ));
        }
        Ok(LibraryScanConfig {
            worker_total,
            enum_workers,
            process_workers,
            seed_depth: self.seed_depth,
            index_queue_capacity: self.index_queue_capacity.max(1) as usize,
            path_queue_capacity: self.path_queue_capacity.max(1) as usize,
            debug,
        })
    }
}

pub fn ui_defaults_from_config(_config: &AppConfig) -> UiPreferences {
    UiPreferences::default()
}

pub fn converter_defaults() -> ConverterSettings {
    ConverterSettings::default()
}

pub fn library_scan_defaults(config: &AppConfig) -> LibraryScanSettings {
    LibraryScanSettings::from_config(&config.library_scan)
}

pub fn downloads_defaults(config: &AppConfig) -> DownloadsSettings {
    DownloadsSettings {
        concurrency: config.download_concurrency as u32,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageSettings {
    pub library: Option<StorageLocation>,
}

impl StorageSettings {
    pub fn local(path: impl Into<String>) -> Self {
        Self {
            library: Some(StorageLocation::Local { path: path.into() }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageLocation {
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password_encrypted: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workgroup: Option<String>,
    },
}

fn default_smb_port() -> u16 {
    445
}

pub fn storage_defaults(_config: &AppConfig) -> StorageSettings {
    StorageSettings::default()
}

pub async fn load_runtime_settings(pool: &SqlitePool, config: &AppConfig) -> RuntimeSettings {
    RuntimeSettings {
        ui: load_ui(pool, config).await,
        converter: load_converter(pool).await,
        library_scan: load_library_scan(pool, config).await,
        downloads: load_downloads(pool, config).await,
        storage: load_storage(pool, config).await,
    }
}

pub async fn load_ui(pool: &SqlitePool, config: &AppConfig) -> UiPreferences {
    load_json(pool, KEY_UI_PREFERENCES, ui_defaults_from_config(config)).await
}

pub async fn load_converter(pool: &SqlitePool) -> ConverterSettings {
    load_json(pool, KEY_CONVERTER_SETTINGS, converter_defaults()).await
}

pub async fn load_library_scan(pool: &SqlitePool, config: &AppConfig) -> LibraryScanSettings {
    load_json(
        pool,
        KEY_LIBRARY_SCAN_SETTINGS,
        library_scan_defaults(config),
    )
    .await
}

pub async fn load_downloads(pool: &SqlitePool, config: &AppConfig) -> DownloadsSettings {
    load_json(pool, KEY_DOWNLOADS_SETTINGS, downloads_defaults(config)).await
}

pub async fn load_storage(pool: &SqlitePool, config: &AppConfig) -> StorageSettings {
    load_json(pool, KEY_STORAGE_SETTINGS, storage_defaults(config)).await
}

async fn load_json<T>(pool: &SqlitePool, key: &str, default: T) -> T
where
    T: for<'de> Deserialize<'de>,
{
    let Some(raw) = settings::get(pool, key).await.ok().flatten() else {
        return default;
    };
    serde_json::from_str(&raw).unwrap_or(default)
}

pub async fn save_ui(pool: &SqlitePool, value: &UiPreferences) -> Result<(), ApiError> {
    validate_ui(value)?;
    save_json(pool, KEY_UI_PREFERENCES, value).await
}

pub async fn save_converter(pool: &SqlitePool, value: &ConverterSettings) -> Result<(), ApiError> {
    validate_converter(value)?;
    save_json(pool, KEY_CONVERTER_SETTINGS, value).await
}

pub async fn save_library_scan(
    pool: &SqlitePool,
    value: &LibraryScanSettings,
    debug: bool,
) -> Result<(), ApiError> {
    value.to_config(debug)?;
    save_json(pool, KEY_LIBRARY_SCAN_SETTINGS, value).await
}

pub async fn save_downloads(pool: &SqlitePool, value: &DownloadsSettings) -> Result<(), ApiError> {
    validate_downloads(value)?;
    save_json(pool, KEY_DOWNLOADS_SETTINGS, value).await
}

pub async fn save_storage(pool: &SqlitePool, value: &StorageSettings) -> Result<(), ApiError> {
    validate_storage(value)?;
    save_json(pool, KEY_STORAGE_SETTINGS, value).await
}

async fn save_json<T>(pool: &SqlitePool, key: &str, value: &T) -> Result<(), ApiError>
where
    T: Serialize,
{
    let raw = serde_json::to_string(value)
        .map_err(|e| ApiError::Message(format!("settings encode: {e}")))?;
    settings::set(pool, key, &raw).await
}

pub fn validate_ui(v: &UiPreferences) -> Result<(), ApiError> {
    if !matches!(v.default_quality, 5 | 6 | 7 | 27) {
        return Err(ApiError::bad_request(
            "default_quality must be one of 5, 6, 7, 27",
        ));
    }
    Ok(())
}

pub fn validate_converter(v: &ConverterSettings) -> Result<(), ApiError> {
    if v.parallelism == 0 || v.parallelism > 32 {
        return Err(ApiError::bad_request("parallelism must be 1..=32"));
    }
    let flac: FlacEncodeSettings = (&v.flac_encode).into();
    flac.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(())
}

pub fn validate_downloads(v: &DownloadsSettings) -> Result<(), ApiError> {
    if v.concurrency == 0 || v.concurrency > 32 {
        return Err(ApiError::bad_request("concurrency must be 1..=32"));
    }
    Ok(())
}

pub fn validate_storage(v: &StorageSettings) -> Result<(), ApiError> {
    match &v.library {
        None => {}
        Some(StorageLocation::Local { path }) if path.trim().is_empty() => {
                return Err(ApiError::bad_request(
                    "local library path must not be empty",
                ));
            }
            Some(StorageLocation::Local { .. }) => {}
        Some(StorageLocation::Smb {
            host, port, share, ..
        }) => {
            if host.trim().is_empty() {
                return Err(ApiError::bad_request("smb host must not be empty"));
            }
            if share.trim().is_empty() {
                return Err(ApiError::bad_request("smb share must not be empty"));
            }
            if *port == 0 {
                return Err(ApiError::bad_request("smb port must be > 0"));
            }
        }
    }
    Ok(())
}

pub type RuntimeSettingsHandle = Arc<RwLock<RuntimeSettings>>;

pub async fn require_local_library_path(
    handle: &RuntimeSettingsHandle,
) -> Result<std::path::PathBuf, ApiError> {
    match &handle.read().await.storage.library {
        Some(StorageLocation::Local { path }) => Ok(std::path::PathBuf::from(path)),
        Some(StorageLocation::Smb { .. }) => Err(ApiError::Message(
            "SMB_LIBRARY_UNSUPPORTED: this library operation still requires local storage".into(),
        )),
        None => Err(ApiError::Message(
            "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
        )),
    }
}

pub async fn refresh_runtime(
    handle: &RuntimeSettingsHandle,
    pool: &SqlitePool,
    config: &AppConfig,
) {
    let loaded = load_runtime_settings(pool, config).await;
    *handle.write().await = loaded;
}

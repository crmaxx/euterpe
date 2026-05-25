use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use std::path::{Component, Path};

use crate::api::{
    ConverterSettingsPatch, ConverterSettingsResponse, DownloadsSettingsPatch,
    DownloadsSettingsResponse, LibraryScanSettingsPatch, LibraryScanSettingsResponse,
    SmbSharesRequest, SmbSharesResponse, StorageBrowseEntry, StorageBrowseResponse,
    StorageLocationPatch, StorageSettingsPatch, StorageSettingsResponse, StorageSettingsView,
    StorageTestRequest, StorageTestResponse, UiPreferencesPatch, UiPreferencesResponse,
};
use crate::error::ApiError;
use crate::library::storage::{self, StorageEntryKind, StoragePath};
use crate::services::app_settings::{self, StorageLocation, StorageSettings};
use crate::state::AppState;

pub async fn get_ui_settings(
    State(state): State<AppState>,
) -> Result<Json<UiPreferencesResponse>, ApiError> {
    let settings = state.runtime.read().await.ui.clone();
    Ok(Json(UiPreferencesResponse { settings }))
}

pub async fn patch_ui_settings(
    State(state): State<AppState>,
    Json(patch): Json<UiPreferencesPatch>,
) -> Result<Json<UiPreferencesResponse>, ApiError> {
    let mut settings = state.runtime.read().await.ui.clone();
    if let Some(v) = patch.theme {
        settings.theme = v;
    }
    if let Some(v) = patch.locale {
        settings.locale = v;
    }
    if let Some(v) = patch.default_quality {
        settings.default_quality = v;
    }
    app_settings::save_ui(&state.db, &settings).await?;
    state.runtime.write().await.ui = settings.clone();
    Ok(Json(UiPreferencesResponse { settings }))
}

pub async fn get_converter_settings(
    State(state): State<AppState>,
) -> Result<Json<ConverterSettingsResponse>, ApiError> {
    let settings = state.runtime.read().await.converter.clone();
    Ok(Json(ConverterSettingsResponse { settings }))
}

pub async fn patch_converter_settings(
    State(state): State<AppState>,
    Json(patch): Json<ConverterSettingsPatch>,
) -> Result<Json<ConverterSettingsResponse>, ApiError> {
    let mut settings = state.runtime.read().await.converter.clone();
    if let Some(v) = patch.auto_enabled {
        settings.auto_enabled = v;
    }
    if let Some(v) = patch.file_policy {
        settings.file_policy = v;
    }
    if let Some(v) = patch.parallelism {
        settings.parallelism = v;
    }
    if let Some(v) = patch.formats {
        settings.formats = v;
    }
    if let Some(patch_flac) = patch.flac_encode {
        if let Some(v) = patch_flac.preset {
            settings.flac_encode.preset = v;
        }
        if let Some(v) = patch_flac.block_size {
            settings.flac_encode.block_size = v;
        }
        if let Some(v) = patch_flac.multithread {
            settings.flac_encode.multithread = v;
        }
    }
    app_settings::save_converter(&state.db, &settings).await?;
    state.runtime.write().await.converter = settings.clone();
    Ok(Json(ConverterSettingsResponse { settings }))
}

pub async fn get_library_scan_settings(
    State(state): State<AppState>,
) -> Result<Json<LibraryScanSettingsResponse>, ApiError> {
    let settings = state.runtime.read().await.library_scan.clone();
    Ok(Json(LibraryScanSettingsResponse { settings }))
}

pub async fn patch_library_scan_settings(
    State(state): State<AppState>,
    Json(patch): Json<LibraryScanSettingsPatch>,
) -> Result<Json<LibraryScanSettingsResponse>, ApiError> {
    let mut settings = state.runtime.read().await.library_scan.clone();
    if let Some(v) = patch.worker_total {
        settings.worker_total = v;
    }
    if let Some(v) = patch.enum_workers {
        settings.enum_workers = v;
    }
    if let Some(v) = patch.process_workers {
        settings.process_workers = v;
    }
    if let Some(v) = patch.seed_depth {
        settings.seed_depth = v;
    }
    if let Some(v) = patch.index_queue_capacity {
        settings.index_queue_capacity = v;
    }
    if let Some(v) = patch.path_queue_capacity {
        settings.path_queue_capacity = v;
    }
    app_settings::save_library_scan(&state.db, &settings, state.config.debug).await?;
    state.runtime.write().await.library_scan = settings.clone();
    Ok(Json(LibraryScanSettingsResponse { settings }))
}

pub async fn get_downloads_settings(
    State(state): State<AppState>,
) -> Result<Json<DownloadsSettingsResponse>, ApiError> {
    let settings = state.runtime.read().await.downloads.clone();
    Ok(Json(DownloadsSettingsResponse { settings }))
}

pub async fn patch_downloads_settings(
    State(state): State<AppState>,
    Json(patch): Json<DownloadsSettingsPatch>,
) -> Result<Json<DownloadsSettingsResponse>, ApiError> {
    let mut settings = state.runtime.read().await.downloads.clone();
    if let Some(v) = patch.concurrency {
        settings.concurrency = v;
    }
    app_settings::save_downloads(&state.db, &settings).await?;
    state.runtime.write().await.downloads = settings.clone();
    Ok(Json(DownloadsSettingsResponse { settings }))
}

pub async fn get_storage_settings(
    State(state): State<AppState>,
) -> Result<Json<StorageSettingsResponse>, ApiError> {
    let settings = state.runtime.read().await.storage.clone();
    let watch_status = state.storage_watch.status().await;
    Ok(Json(StorageSettingsResponse {
        settings: StorageSettingsView::from_with_watch_status(&settings, watch_status),
    }))
}

pub async fn patch_storage_settings(
    State(state): State<AppState>,
    Json(patch): Json<StorageSettingsPatch>,
) -> Result<Json<StorageSettingsResponse>, ApiError> {
    let settings = storage_patch_to_settings(&state, patch).await?;
    app_settings::save_storage(&state.db, &settings).await?;
    state.runtime.write().await.storage = settings.clone();
    state.storage_watch.restart().await;
    let watch_status = state.storage_watch.status().await;
    Ok(Json(StorageSettingsResponse {
        settings: StorageSettingsView::from_with_watch_status(&settings, watch_status),
    }))
}

pub async fn test_storage_settings(
    State(state): State<AppState>,
    Json(body): Json<StorageTestRequest>,
) -> Result<Json<StorageTestResponse>, ApiError> {
    let settings = storage_patch_to_settings(
        &state,
        StorageSettingsPatch {
            library: body.location,
        },
    )
    .await?;
    match &settings.library {
        None => {
            return Err(ApiError::bad_request("library storage is not configured"));
        }
        Some(StorageLocation::Local { path }) => {
            let meta = tokio::fs::metadata(path)
                .await
                .map_err(|e| ApiError::Message(format!("local storage not available: {e}")))?;
            if !meta.is_dir() {
                return Err(ApiError::bad_request(
                    "local storage path is not a directory",
                ));
            }
        }
        Some(StorageLocation::Smb { .. }) => {
            let (location, credentials) =
                smb_location_and_credentials(&state, settings.library.as_ref().unwrap())?;
            euterpe_smb::SmbStorageClient::new()
                .list_directory(&location, &credentials)
                .await
                .map_err(|e| ApiError::Message(format!("SMB_STORAGE_TEST_FAILED: {e}")))?;
        }
    }
    Ok(Json(StorageTestResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
pub struct StorageBrowseQuery {
    pub target: String,
    #[serde(default)]
    pub path: Option<String>,
}

pub async fn browse_storage(
    State(state): State<AppState>,
    Query(q): Query<StorageBrowseQuery>,
) -> Result<Json<StorageBrowseResponse>, ApiError> {
    if q.target != "library" {
        return Err(ApiError::bad_request("only target=library is supported"));
    }
    let storage = state.runtime.read().await.storage.clone();
    let library = storage
        .library
        .ok_or_else(|| ApiError::bad_request("library storage is not configured"))?;
    let backend = storage::storage_from_location(&library, state.config.master_key.as_ref())?;
    let rel = StoragePath::parse(normalize_browse_path(q.path.as_deref())?)?;
    let entries = backend
        .list_dir(&rel)
        .await?
        .into_iter()
        .map(|entry| StorageBrowseEntry {
            name: entry.name,
            path: entry.path.as_str().to_string(),
            is_dir: entry.kind == StorageEntryKind::Directory,
            size: entry.size,
        })
        .collect();
    Ok(Json(StorageBrowseResponse { entries }))
}

pub async fn list_smb_shares(
    Json(body): Json<SmbSharesRequest>,
) -> Result<Json<SmbSharesResponse>, ApiError> {
    let username = body.username.unwrap_or_default();
    let password = body.password.unwrap_or_default();
    let shares = euterpe_smb::SmbStorageClient::new()
        .list_shares(
            &body.host,
            body.port,
            &euterpe_smb::SmbCredentials { username, password },
        )
        .await
        .map_err(|e| ApiError::Message(format!("SMB_SHARES_FAILED: {e}")))?;
    Ok(Json(SmbSharesResponse { shares }))
}

async fn storage_patch_to_settings(
    state: &AppState,
    patch: StorageSettingsPatch,
) -> Result<StorageSettings, ApiError> {
    let library = match patch.library {
        StorageLocationPatch::Local { path } => StorageLocation::Local { path },
        StorageLocationPatch::Smb {
            host,
            port,
            share,
            path,
            username,
            password,
            workgroup,
        } => {
            let current_password = match &state.runtime.read().await.storage.library {
                Some(StorageLocation::Smb {
                    password_encrypted, ..
                }) => password_encrypted.clone(),
                _ => None,
            };
            let password_encrypted = match password {
                Some(password) if !password.is_empty() => {
                    Some(state.master_key()?.encrypt(&password)?)
                }
                Some(_) => None,
                None => current_password,
            };
            StorageLocation::Smb {
                host,
                port,
                share,
                path: normalize_browse_path(Some(&path))?,
                username,
                password_encrypted,
                workgroup,
            }
        }
    };
    let settings = StorageSettings {
        library: Some(library),
    };
    app_settings::validate_storage(&settings)?;
    Ok(settings)
}

fn normalize_browse_path(path: Option<&str>) -> Result<String, ApiError> {
    let Some(path) = path else {
        return Ok(String::new());
    };
    let normalized = path.replace('\\', "/");
    if normalized.trim().is_empty() {
        return Ok(String::new());
    }
    let rel = Path::new(&normalized);
    if rel.is_absolute() {
        return Err(ApiError::bad_request("storage path must be relative"));
    }
    let mut parts = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(ApiError::bad_request("storage path must not contain .."));
            }
            _ => return Err(ApiError::bad_request("invalid storage path")),
        }
    }
    Ok(parts.join("/"))
}

fn smb_location_and_credentials(
    state: &AppState,
    storage: &StorageLocation,
) -> Result<(euterpe_smb::SmbShareLocation, euterpe_smb::SmbCredentials), ApiError> {
    let StorageLocation::Smb {
        host,
        port,
        share,
        path,
        username,
        password_encrypted,
        ..
    } = storage
    else {
        return Err(ApiError::bad_request("storage location is not smb"));
    };
    let password = match password_encrypted {
        Some(value) => state.master_key()?.decrypt(value)?,
        None => String::new(),
    };
    Ok((
        euterpe_smb::SmbShareLocation {
            host: host.clone(),
            port: *port,
            share: share.clone(),
            path: path.clone(),
        },
        euterpe_smb::SmbCredentials {
            username: username.clone().unwrap_or_default(),
            password,
        },
    ))
}

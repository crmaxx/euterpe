use axum::Json;
use axum::extract::{Query, State};
use euterpe_data::repositories::qobuz as qobuz_runs;
use euterpe_data::repositories::qobuz::QobuzSyncTrigger;
use serde::Deserialize;
use std::path::{Component, Path};

use crate::api::{
    ConverterSettingsPatch, ConverterSettingsResponse, DownloadsSettingsPatch,
    DownloadsSettingsResponse, LibraryScanSettingsPatch, LibraryScanSettingsResponse,
    QobuzScheduledSyncSettingsPatch, QobuzScheduledSyncSettingsResponse, QobuzScheduledSyncStatus,
    QobuzSyncRunSummary, SmbSharesRequest, SmbSharesResponse, StorageBrowseEntry,
    StorageBrowseRequest, StorageBrowseResponse, StorageLocationPatch, StorageSettingsPatch,
    StorageSettingsResponse, StorageSettingsView, StorageTestRequest, StorageTestResponse,
    StringPatchField, UiPreferencesPatch, UiPreferencesResponse,
};
use crate::error::ApiError;
use crate::library::storage::{self, StorageEntryKind, StoragePath};
use crate::services::app_settings::{self, StorageLocation, StorageSettings};
use crate::services::qobuz_scheduled_sync::{CronSchedule, server_timezone_label};
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
    app_settings::save_ui(&state.data, &settings).await?;
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
    app_settings::save_converter(&state.data, &settings).await?;
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
    app_settings::save_library_scan(&state.data, &settings, state.config.debug).await?;
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
    app_settings::save_downloads(&state.data, &settings).await?;
    state.runtime.write().await.downloads = settings.clone();
    Ok(Json(DownloadsSettingsResponse { settings }))
}

pub async fn get_qobuz_scheduled_sync_settings(
    State(state): State<AppState>,
) -> Result<Json<QobuzScheduledSyncSettingsResponse>, ApiError> {
    let settings = state.runtime.read().await.qobuz_scheduled_sync.clone();
    qobuz_scheduled_sync_response(&state, settings)
        .await
        .map(Json)
}

pub async fn patch_qobuz_scheduled_sync_settings(
    State(state): State<AppState>,
    Json(patch): Json<QobuzScheduledSyncSettingsPatch>,
) -> Result<Json<QobuzScheduledSyncSettingsResponse>, ApiError> {
    let mut settings = state.runtime.read().await.qobuz_scheduled_sync.clone();
    if let Some(v) = patch.enabled {
        settings.enabled = v;
    }
    if let Some(v) = patch.cron_expression {
        settings.cron_expression = v;
    }
    if let Some(v) = patch.auto_download_new_favorites {
        settings.auto_download_new_favorites = v;
    }
    settings = app_settings::normalize_qobuz_scheduled_sync(settings);
    app_settings::save_qobuz_scheduled_sync(&state.data, &settings).await?;
    state.runtime.write().await.qobuz_scheduled_sync = settings.clone();
    state.qobuz_scheduled_sync.restart().await?;
    qobuz_scheduled_sync_response(&state, settings)
        .await
        .map(Json)
}

pub async fn run_qobuz_scheduled_sync_now(
    State(state): State<AppState>,
) -> Result<Json<QobuzScheduledSyncSettingsResponse>, ApiError> {
    state.require_credentials().await?;
    let settings = state.runtime.read().await.qobuz_scheduled_sync.clone();
    state
        .qobuz_scheduled_sync
        .trigger_once(QobuzSyncTrigger::SettingsRunNow)
        .await?;
    qobuz_scheduled_sync_response(&state, settings)
        .await
        .map(Json)
}

async fn qobuz_scheduled_sync_response(
    state: &AppState,
    settings: app_settings::QobuzScheduledSyncSettings,
) -> Result<QobuzScheduledSyncSettingsResponse, ApiError> {
    let next_run_at = if settings.enabled {
        Some(
            CronSchedule::parse(&settings.cron_expression)?
                .next_from_now()?
                .to_rfc3339(),
        )
    } else {
        None
    };
    let last_run = qobuz_runs::sync_latest(&state.data)
        .await?
        .map(qobuz_sync_run_from_data);
    Ok(QobuzScheduledSyncSettingsResponse {
        settings,
        status: QobuzScheduledSyncStatus {
            server_timezone: server_timezone_label(),
            next_run_at,
            last_run,
        },
    })
}

fn qobuz_sync_run_from_data(row: qobuz_runs::QobuzSyncRunSummary) -> QobuzSyncRunSummary {
    QobuzSyncRunSummary {
        id: row.id,
        status: row.status,
        trigger: row.trigger,
        started_at: row.started_at,
        finished_at: row.finished_at,
        albums_total: row.albums_total,
        albums_added: row.albums_added,
        albums_removed: row.albums_removed,
        error_message: row.error_message,
        skip_reason: row.skip_reason,
    }
}

pub async fn get_storage_settings(
    State(state): State<AppState>,
) -> Result<Json<StorageSettingsResponse>, ApiError> {
    let settings = state.runtime.read().await.storage.clone();
    let watch_status = state.storage_watch.status().await;
    Ok(Json(storage_settings_response(
        &settings,
        watch_status,
        None,
    )))
}

pub async fn patch_storage_settings(
    State(state): State<AppState>,
    Json(patch): Json<StorageSettingsPatch>,
) -> Result<Json<StorageSettingsResponse>, ApiError> {
    let previous = state.runtime.read().await.storage.clone();
    let settings = storage_patch_to_settings(&state, patch).await?;
    let migration = storage_kind_migration(&previous, &settings);
    app_settings::save_storage(&state.data, &settings).await?;
    state.runtime.write().await.storage = settings.clone();
    state.invalidate_library_storage_cache().await;
    state.storage_watch.restart().await;
    let watch_status = state.storage_watch.status().await;
    Ok(Json(storage_settings_response(
        &settings,
        watch_status,
        migration,
    )))
}

fn storage_settings_response(
    settings: &StorageSettings,
    watch_status: crate::services::storage_watch::StorageWatchStatus,
    migration: Option<(String, bool)>,
) -> StorageSettingsResponse {
    let (storage_migration_hint, recommend_full_scan) = match migration {
        Some((hint, recommend)) => (Some(hint), Some(recommend)),
        None => (None, None),
    };
    StorageSettingsResponse {
        settings: StorageSettingsView::from_with_watch_status(settings, watch_status),
        storage_migration_hint,
        recommend_full_scan,
    }
}

fn storage_location_kind(location: &StorageLocation) -> &'static str {
    match location {
        StorageLocation::Local { .. } => "local",
        StorageLocation::Smb { .. } => "smb",
    }
}

fn storage_kind_migration(
    previous: &StorageSettings,
    next: &StorageSettings,
) -> Option<(String, bool)> {
    let (Some(old), Some(new)) = (&previous.library, &next.library) else {
        return None;
    };
    let old_kind = storage_location_kind(old);
    let new_kind = storage_location_kind(new);
    if old_kind == new_kind {
        return None;
    }
    let hint = match (old_kind, new_kind) {
        ("local", "smb") => {
            "Library storage switched from local disk to SMB. Run a full library scan to rebuild the index."
        }
        ("smb", "local") => {
            "Library storage switched from SMB to local disk. Run a full library scan to rebuild the index."
        }
        _ => "Library storage backend changed. Run a full library scan to rebuild the index.",
    };
    Some((hint.to_string(), true))
}

pub async fn test_storage_settings(
    State(state): State<AppState>,
    Json(body): Json<StorageTestRequest>,
) -> Result<Json<StorageTestResponse>, ApiError> {
    let settings = storage_patch_to_settings(
        &state,
        StorageSettingsPatch {
            activate_preset_id: None,
            library: Some(body.location),
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
                .map_err(|e| ApiError::from_smb(e, "SMB storage test"))?;
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
    browse_storage_location(&state, &library, q.path.as_deref()).await
}

pub async fn browse_storage_draft(
    State(state): State<AppState>,
    Json(body): Json<StorageBrowseRequest>,
) -> Result<Json<StorageBrowseResponse>, ApiError> {
    let current = state.runtime.read().await.storage.clone();
    let library = storage_location_patch_to_location(&state, &current, body.location).await?;
    browse_storage_location(&state, &library, body.path.as_deref()).await
}

async fn browse_storage_location(
    state: &AppState,
    library: &StorageLocation,
    path: Option<&str>,
) -> Result<Json<StorageBrowseResponse>, ApiError> {
    let backend = storage::storage_from_location(library, state.config.master_key.as_ref())?;
    let rel = StoragePath::parse(normalize_browse_path(path)?)?;
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
    State(state): State<AppState>,
    Json(body): Json<SmbSharesRequest>,
) -> Result<Json<SmbSharesResponse>, ApiError> {
    let credentials = smb_shares_credentials(&state, &body).await?;
    let shares = euterpe_smb::SmbStorageClient::new()
        .list_shares(&body.host, body.port, &credentials)
        .await
        .map_err(|e| ApiError::from_smb(e, "SMB share list"))?;
    Ok(Json(SmbSharesResponse { shares }))
}

/// Merges request credentials with the saved library SMB location when password is omitted.
async fn smb_shares_credentials(
    state: &AppState,
    body: &SmbSharesRequest,
) -> Result<euterpe_smb::SmbCredentials, ApiError> {
    let storage = state.runtime.read().await.storage.clone();
    let stored = storage.library.as_ref().and_then(|loc| match loc {
        StorageLocation::Smb { .. } => Some(loc),
        _ => None,
    });
    let decrypt = |encrypted: &str| state.master_key()?.decrypt(encrypted);
    merge_smb_shares_credentials(body, stored, decrypt)
}

fn merge_smb_shares_credentials(
    body: &SmbSharesRequest,
    stored: Option<&StorageLocation>,
    decrypt_password: impl FnOnce(&str) -> Result<String, ApiError>,
) -> Result<euterpe_smb::SmbCredentials, ApiError> {
    let password = match body.password.as_deref() {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => match stored {
            Some(StorageLocation::Smb {
                host,
                port,
                password_encrypted: Some(encrypted),
                ..
            }) if hosts_match(host, &body.host) && *port == body.port => {
                decrypt_password(encrypted)?
            }
            _ => String::new(),
        },
    };

    let (username, workgroup) = if body.username.as_deref().is_some_and(|u| !u.is_empty()) {
        (body.username.clone(), body.workgroup.clone())
    } else {
        match stored {
            Some(StorageLocation::Smb {
                host,
                port,
                username,
                workgroup,
                ..
            }) if hosts_match(host, &body.host) && *port == body.port => {
                (username.clone(), workgroup.clone())
            }
            _ => (body.username.clone(), body.workgroup.clone()),
        }
    };

    Ok(euterpe_smb::SmbCredentials {
        username: euterpe_smb::format_smb_username(
            workgroup.as_deref(),
            username.as_deref().unwrap_or_default(),
        ),
        password,
    })
}

fn hosts_match(stored: &str, requested: &str) -> bool {
    stored.eq_ignore_ascii_case(requested)
}

fn smb_endpoint_matches(
    stored_host: &str,
    stored_port: u16,
    stored_share: &str,
    requested_host: &str,
    requested_port: u16,
    requested_share: &str,
) -> bool {
    hosts_match(stored_host, requested_host)
        && stored_port == requested_port
        && stored_share.eq_ignore_ascii_case(requested_share)
}

fn current_smb_defaults_for_patch(
    current: &StorageSettings,
    requested_host: &str,
    requested_port: u16,
    requested_share: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    match &current.library {
        Some(StorageLocation::Smb {
            host,
            port,
            share,
            password_encrypted,
            username,
            workgroup,
            ..
        }) => {
            let password = if smb_endpoint_matches(
                host,
                *port,
                share,
                requested_host,
                requested_port,
                requested_share,
            ) {
                password_encrypted.clone()
            } else {
                None
            };
            (password, username.clone(), workgroup.clone())
        }
        _ => (None, None, None),
    }
}

async fn storage_patch_to_settings(
    state: &AppState,
    patch: StorageSettingsPatch,
) -> Result<StorageSettings, ApiError> {
    let current = state.runtime.read().await.storage.clone();
    let library = if let Some(preset_id) = patch.activate_preset_id {
        current
            .presets
            .iter()
            .find(|preset| preset.id == preset_id)
            .map(|preset| preset.location.clone())
            .ok_or_else(|| ApiError::bad_request("unknown storage preset"))?
    } else if let Some(library_patch) = patch.library {
        storage_location_patch_to_location(state, &current, library_patch).await?
    } else {
        return Err(ApiError::bad_request(
            "library or activate_preset_id is required",
        ));
    };
    let mut settings = current;
    app_settings::upsert_storage_preset(&mut settings.presets, library.clone());
    settings.library = Some(library);
    app_settings::validate_storage(&settings)?;
    Ok(settings)
}

async fn storage_location_patch_to_location(
    state: &AppState,
    current: &StorageSettings,
    patch: StorageLocationPatch,
) -> Result<StorageLocation, ApiError> {
    match patch {
        StorageLocationPatch::Local { path } => Ok(StorageLocation::Local { path }),
        StorageLocationPatch::Smb {
            host,
            port,
            share,
            path,
            username,
            password,
            workgroup,
        } => {
            let (current_password, current_username, current_workgroup) =
                current_smb_defaults_for_patch(current, &host, port, &share);
            let password_encrypted = match password {
                StringPatchField::Value(password) if !password.is_empty() => {
                    Some(state.master_key()?.encrypt(&password)?)
                }
                StringPatchField::Value(_) | StringPatchField::Clear => None,
                StringPatchField::Missing => current_password,
            };
            let username = match username {
                StringPatchField::Value(value) if !value.is_empty() => Some(value),
                StringPatchField::Value(_) | StringPatchField::Clear => None,
                StringPatchField::Missing => current_username,
            };
            let workgroup = match workgroup {
                StringPatchField::Value(value) if !value.is_empty() => Some(value),
                StringPatchField::Value(_) | StringPatchField::Clear => None,
                StringPatchField::Missing => current_workgroup,
            };
            Ok(StorageLocation::Smb {
                host,
                port,
                share,
                path: normalize_browse_path(Some(&path))?,
                username,
                password_encrypted,
                workgroup,
            })
        }
    }
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
        workgroup,
    } = storage
    else {
        return Err(ApiError::bad_request("storage location is not smb"));
    };
    let password = match password_encrypted {
        Some(value) => state.master_key()?.decrypt(value)?,
        None => String::new(),
    };
    let username = euterpe_smb::format_smb_username(
        workgroup.as_deref(),
        username.as_deref().unwrap_or_default(),
    );
    Ok((
        euterpe_smb::SmbShareLocation {
            host: host.clone(),
            port: *port,
            share: share.clone(),
            path: path.clone(),
        },
        euterpe_smb::SmbCredentials { username, password },
    ))
}

#[cfg(test)]
mod smb_shares_credentials_tests {
    use super::*;
    use crate::api::SmbSharesRequest;

    #[test]
    fn uses_stored_password_when_request_omits_it() {
        let body = SmbSharesRequest {
            host: "192.168.0.124".into(),
            port: 445,
            username: Some("dietpi".into()),
            password: None,
            workgroup: None,
        };
        let stored = StorageLocation::Smb {
            host: "192.168.0.124".into(),
            port: 445,
            share: "music".into(),
            path: String::new(),
            username: Some("dietpi".into()),
            password_encrypted: Some("enc".into()),
            workgroup: None,
        };
        let creds =
            merge_smb_shares_credentials(&body, Some(&stored), |_| Ok("secret".into())).unwrap();
        assert_eq!(creds.password, "secret");
        assert!(creds.username.contains("dietpi"));
    }

    #[test]
    fn request_password_overrides_stored() {
        let body = SmbSharesRequest {
            host: "192.168.0.124".into(),
            port: 445,
            username: None,
            password: Some("inline".into()),
            workgroup: None,
        };
        let stored = StorageLocation::Smb {
            host: "192.168.0.124".into(),
            port: 445,
            share: "music".into(),
            path: String::new(),
            username: None,
            password_encrypted: Some("enc".into()),
            workgroup: None,
        };
        let creds =
            merge_smb_shares_credentials(&body, Some(&stored), |_| Ok("stored".into())).unwrap();
        assert_eq!(creds.password, "inline");
    }

    #[test]
    fn storage_patch_reuses_password_only_for_same_endpoint() {
        let current = StorageSettings {
            library: Some(StorageLocation::Smb {
                host: "NAS.local".into(),
                port: 445,
                share: "music".into(),
                path: String::new(),
                username: Some("user".into()),
                password_encrypted: Some("enc".into()),
                workgroup: Some("WORKGROUP".into()),
            }),
            presets: Vec::new(),
        };

        let same = current_smb_defaults_for_patch(&current, "nas.LOCAL", 445, "MUSIC");
        assert_eq!(same.0, Some("enc".into()));
        assert_eq!(same.1, Some("user".into()));
        assert_eq!(same.2, Some("WORKGROUP".into()));

        let changed_share = current_smb_defaults_for_patch(&current, "NAS.local", 445, "archive");
        assert_eq!(changed_share.0, None);
        assert_eq!(changed_share.1, Some("user".into()));
        assert_eq!(changed_share.2, Some("WORKGROUP".into()));

        let changed_port = current_smb_defaults_for_patch(&current, "NAS.local", 1445, "music");
        assert_eq!(changed_port.0, None);
    }
}

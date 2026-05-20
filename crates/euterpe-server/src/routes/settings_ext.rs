use axum::Json;
use axum::extract::State;

use crate::api::{
    ConverterSettingsPatch, ConverterSettingsResponse, DownloadsSettingsPatch,
    DownloadsSettingsResponse, LibraryScanSettingsPatch, LibraryScanSettingsResponse,
    UiPreferencesPatch, UiPreferencesResponse,
};
use crate::error::ApiError;
use crate::services::app_settings;
use crate::state::AppState;

pub async fn get_ui_settings(State(state): State<AppState>) -> Result<Json<UiPreferencesResponse>, ApiError> {
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

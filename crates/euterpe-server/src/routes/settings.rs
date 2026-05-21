use axum::Json;
use axum::extract::State;

use crate::api::{TorrentSettingsPatch, TorrentSettingsResponse};
use crate::error::ApiError;
use crate::services::torrent_settings;
use crate::state::AppState;

pub async fn get_torrent_settings(
    State(state): State<AppState>,
) -> Result<Json<TorrentSettingsResponse>, ApiError> {
    let settings = torrent_settings::load(&state.db).await?;
    Ok(Json(TorrentSettingsResponse { settings }))
}

pub async fn patch_torrent_settings(
    State(state): State<AppState>,
    Json(patch): Json<TorrentSettingsPatch>,
) -> Result<Json<TorrentSettingsResponse>, ApiError> {
    let mut settings = torrent_settings::load(&state.db).await?;
    if let Some(v) = patch.disable_upload {
        settings.disable_upload = v;
    }
    if let Some(v) = patch.max_upload_kib_per_sec {
        settings.max_upload_kib_per_sec = v;
    }
    torrent_settings::validate(&settings)?;

    if let Some(engine) = state.torrent.as_ref() {
        engine
            .apply_session_settings(torrent_settings::to_session_settings(&settings))
            .await
            .map_err(|e| ApiError::Message(e.to_string()))?;
    }

    torrent_settings::save(&state.db, &settings).await?;
    Ok(Json(TorrentSettingsResponse { settings }))
}

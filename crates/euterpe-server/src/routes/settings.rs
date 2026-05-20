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
    if let Some(v) = patch.seed_ratio_limit {
        settings.seed_ratio_limit = v;
    }
    if let Some(v) = patch.seed_time_limit_sec {
        settings.seed_time_limit_sec = v;
    }
    if let Some(v) = patch.max_upload_kib_per_sec {
        settings.max_upload_kib_per_sec = v;
    }
    torrent_settings::save(&state.db, &settings).await?;
    if let Some(engine) = state.torrent.as_ref() {
        engine.apply_ratelimits(torrent_settings::to_session_settings(&settings));
    }
    Ok(Json(TorrentSettingsResponse { settings }))
}

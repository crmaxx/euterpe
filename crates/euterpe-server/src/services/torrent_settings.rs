use std::env;
use std::num::NonZeroU32;

use euterpe_torrent::SessionSettings;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::api::TorrentSettings;
use crate::db::settings;
use crate::error::ApiError;

pub const KEY_TORRENT_SETTINGS: &str = "torrent.settings";

/// Legacy JSON before `disable_upload` was exposed directly.
#[derive(Debug, Deserialize)]
struct TorrentSettingsLegacy {
    #[serde(default)]
    seed_ratio_limit: f64,
    #[serde(default)]
    seed_time_limit_sec: u64,
    #[serde(default)]
    disable_upload: Option<bool>,
    #[serde(default)]
    max_upload_kib_per_sec: u64,
}

pub fn default_from_env() -> TorrentSettings {
    let disable_upload = env::var("EUTERPE_TORRENT_DISABLE_UPLOAD")
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(true);
    TorrentSettings {
        disable_upload,
        max_upload_kib_per_sec: env::var("EUTERPE_TORRENT_DEFAULT_MAX_UPLOAD_KIB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
    }
}

pub async fn load(pool: &SqlitePool) -> Result<TorrentSettings, ApiError> {
    let Some(raw) = settings::get(pool, KEY_TORRENT_SETTINGS).await? else {
        return Ok(default_from_env());
    };
    if let Ok(settings) = serde_json::from_str::<TorrentSettings>(&raw) {
        return Ok(settings);
    }
    let legacy: TorrentSettingsLegacy = serde_json::from_str(&raw)
        .map_err(|e| ApiError::Message(format!("torrent settings: {e}")))?;
    Ok(normalize_legacy(legacy))
}

fn normalize_legacy(legacy: TorrentSettingsLegacy) -> TorrentSettings {
    let disable_upload = legacy.disable_upload.unwrap_or(
        legacy.seed_ratio_limit == 0.0 && legacy.seed_time_limit_sec == 0,
    );
    TorrentSettings {
        disable_upload,
        max_upload_kib_per_sec: legacy.max_upload_kib_per_sec,
    }
}

pub async fn save(pool: &SqlitePool, value: &TorrentSettings) -> Result<(), ApiError> {
    validate(value)?;
    let raw = serde_json::to_string(value)
        .map_err(|e| ApiError::Message(format!("torrent settings encode: {e}")))?;
    settings::set(pool, KEY_TORRENT_SETTINGS, &raw).await
}

pub fn validate(_s: &TorrentSettings) -> Result<(), ApiError> {
    Ok(())
}

pub fn to_session_settings(s: &TorrentSettings) -> SessionSettings {
    let upload_bps = if s.disable_upload || s.max_upload_kib_per_sec == 0 {
        None
    } else {
        NonZeroU32::new((s.max_upload_kib_per_sec * 1024) as u32)
    };
    SessionSettings {
        disable_upload: s.disable_upload,
        upload_bps,
        download_bps: None,
        enable_upnp_port_forwarding: true,
    }
}

pub fn to_limits_config(s: &TorrentSettings) -> euterpe_torrent::LimitsConfig {
    to_session_settings(s).limits_config()
}

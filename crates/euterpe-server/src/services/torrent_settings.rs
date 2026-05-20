use std::env;
use std::num::NonZeroU32;

use euterpe_torrent::SessionSettings;
use sqlx::SqlitePool;

use crate::api::TorrentSettings;
use crate::db::settings;
use crate::error::ApiError;

pub const KEY_TORRENT_SETTINGS: &str = "torrent.settings";

pub fn default_from_env() -> TorrentSettings {
    TorrentSettings {
        seed_ratio_limit: env::var("EUTERPE_TORRENT_DEFAULT_SEED_RATIO")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0),
        seed_time_limit_sec: env::var("EUTERPE_TORRENT_DEFAULT_SEED_TIME_SEC")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
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
    serde_json::from_str(&raw).map_err(|e| ApiError::Message(format!("torrent settings: {e}")))
}

pub async fn save(pool: &SqlitePool, value: &TorrentSettings) -> Result<(), ApiError> {
    validate(value)?;
    let raw = serde_json::to_string(value)
        .map_err(|e| ApiError::Message(format!("torrent settings encode: {e}")))?;
    settings::set(pool, KEY_TORRENT_SETTINGS, &raw).await
}

pub fn validate(s: &TorrentSettings) -> Result<(), ApiError> {
    if s.seed_ratio_limit < 0.0 {
        return Err(ApiError::bad_request("seed_ratio_limit must be >= 0"));
    }
    if s.seed_ratio_limit > 0.0 || s.seed_time_limit_sec > 0 {
        return Err(ApiError::bad_request(
            "librqbit v1 supports download-only preset (seed_ratio_limit=0 and seed_time_limit_sec=0); use max_upload_kib_per_sec to cap upload",
        ));
    }
    Ok(())
}

pub fn to_session_settings(s: &TorrentSettings) -> SessionSettings {
    let upload_bps = if s.max_upload_kib_per_sec == 0 {
        None
    } else {
        NonZeroU32::new((s.max_upload_kib_per_sec * 1024) as u32)
    };
    SessionSettings {
        disable_upload: s.seed_ratio_limit == 0.0 && s.seed_time_limit_sec == 0,
        upload_bps,
        download_bps: None,
        enable_upnp_port_forwarding: true,
    }
}

pub fn to_limits_config(s: &TorrentSettings) -> euterpe_torrent::LimitsConfig {
    to_session_settings(s).limits_config()
}

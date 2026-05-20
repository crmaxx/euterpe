use euterpe_qobuz::{QobuzClient, QobuzConfig};
use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::crypto::MasterKey;
use crate::db::qobuz_accounts;
use crate::db::settings::{self, KEY_QOBUZ_ACTIVE_ACCOUNT_ID};
use crate::error::ApiError;

#[derive(Debug, Clone)]
pub struct QobuzCredentials {
    pub user_id: u64,
    pub auth_token: String,
}

/// Load credentials for the active Qobuz account (`qobuz.active_account_id` → `qobuz_accounts`).
pub async fn load_active(
    config: &AppConfig,
    pool: &SqlitePool,
) -> Result<Option<QobuzCredentials>, ApiError> {
    let Some(account_id_str) = settings::get(pool, KEY_QOBUZ_ACTIVE_ACCOUNT_ID).await? else {
        return Ok(None);
    };
    let account_id = account_id_str
        .parse::<i64>()
        .map_err(|e| ApiError::Config(format!("invalid qobuz.active_account_id: {e}")))?;

    let Some(row) = qobuz_accounts::get_by_id(pool, account_id).await? else {
        return Ok(None);
    };

    let master = config.master_key.as_ref().ok_or_else(|| {
        ApiError::Message("EUTERPE_MASTER_KEY is required for Qobuz accounts".into())
    })?;

    let auth_token = master.decrypt(&row.uat_encrypted)?;

    Ok(Some(QobuzCredentials {
        user_id: row.qobuz_user_id as u64,
        auth_token,
    }))
}

pub async fn build_client(
    creds: &QobuzCredentials,
    app_config: &AppConfig,
) -> Result<QobuzClient, ApiError> {
    let mut config = QobuzConfig::session_token(creds.user_id, creds.auth_token.clone());
    if let Some(play) = &app_config.qobuz_play_base {
        config.play_base = play.clone();
    }
    if let Some(api) = &app_config.qobuz_api_base {
        config.api_base = api.clone();
    }
    Ok(QobuzClient::connect(config).await?)
}

pub async fn connect_ephemeral(user_id: u64, auth_token: &str) -> Result<QobuzClient, ApiError> {
    let config = QobuzConfig::session_token(user_id, auth_token);
    Ok(QobuzClient::connect(config).await?)
}

pub fn membership_label(client: &QobuzClient) -> String {
    if client.is_authenticated() {
        "Qobuz".to_string()
    } else {
        "Unknown".to_string()
    }
}

pub async fn persist_oauth_account(
    pool: &SqlitePool,
    master: &MasterKey,
    login: &euterpe_qobuz::OAuthLoginResult,
) -> Result<i64, ApiError> {
    let enc = master.encrypt(&login.user_auth_token)?;
    let now = chrono::Utc::now();
    let account_id = qobuz_accounts::upsert_after_oauth(
        pool,
        login.user_id as i64,
        &enc,
        login.display_name.as_deref(),
        login.membership_label.as_deref(),
        now,
        None,
    )
    .await?;

    settings::set(pool, KEY_QOBUZ_ACTIVE_ACCOUNT_ID, &account_id.to_string()).await?;

    Ok(account_id)
}

/// Remove the active Qobuz account and clear the active-account setting.
pub async fn disconnect_active(pool: &SqlitePool) -> Result<(), ApiError> {
    let Some(account_id_str) = settings::get(pool, KEY_QOBUZ_ACTIVE_ACCOUNT_ID).await? else {
        return Ok(());
    };
    let account_id = account_id_str
        .parse::<i64>()
        .map_err(|e| ApiError::Config(format!("invalid qobuz.active_account_id: {e}")))?;
    let _ = qobuz_accounts::delete_by_id(pool, account_id).await?;
    settings::delete(pool, KEY_QOBUZ_ACTIVE_ACCOUNT_ID).await?;
    Ok(())
}

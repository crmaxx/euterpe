use euterpe_qobuz::{QobuzClient, QobuzConfig};
use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::crypto::MasterKey;
use crate::db::settings::{self, KEY_QOBUZ_UAT_ENC, KEY_QOBUZ_USER_ID};
use crate::error::ApiError;

#[derive(Debug, Clone)]
pub struct QobuzCredentials {
    pub user_id: u64,
    pub auth_token: String,
}

pub async fn load_from_env_or_db(
    config: &AppConfig,
    pool: &SqlitePool,
) -> Result<Option<QobuzCredentials>, ApiError> {
    if let (Some(user_id), Some(token)) = (config.qobuz_user_id, &config.qobuz_auth_token) {
        return Ok(Some(QobuzCredentials {
            user_id,
            auth_token: token.clone(),
        }));
    }

    let user_id_str = settings::get(pool, KEY_QOBUZ_USER_ID).await?;
    let uat_enc = settings::get(pool, KEY_QOBUZ_UAT_ENC).await?;
    let (Some(user_id_str), Some(uat_enc)) = (user_id_str, uat_enc) else {
        return Ok(None);
    };

    let master = config
        .master_key
        .as_ref()
        .ok_or_else(|| ApiError::Message("Qobuz credentials in DB require EUTERPE_MASTER_KEY".into()))?;

    let user_id = user_id_str
        .parse::<u64>()
        .map_err(|e| ApiError::Config(format!("invalid stored qobuz.user_id: {e}")))?;
    let auth_token = master.decrypt(&uat_enc)?;

    Ok(Some(QobuzCredentials {
        user_id,
        auth_token,
    }))
}

pub async fn persist(
    pool: &SqlitePool,
    master: &MasterKey,
    creds: &QobuzCredentials,
) -> Result<(), ApiError> {
    let enc = master.encrypt(&creds.auth_token)?;
    settings::set(pool, KEY_QOBUZ_USER_ID, &creds.user_id.to_string()).await?;
    settings::set(pool, KEY_QOBUZ_UAT_ENC, &enc).await?;
    Ok(())
}

pub async fn build_client(creds: &QobuzCredentials) -> Result<QobuzClient, ApiError> {
    let config = QobuzConfig::session_token(creds.user_id, creds.auth_token.clone());
    Ok(QobuzClient::connect(config).await?)
}

pub async fn connect_ephemeral(user_id: u64, auth_token: &str) -> Result<QobuzClient, ApiError> {
    let config = QobuzConfig::session_token(user_id, auth_token);
    Ok(QobuzClient::connect(config).await?)
}

pub fn membership_label(client: &QobuzClient) -> String {
    // Session verify does not return profile; use generic label until user/login refresh.
    if client.is_authenticated() {
        "Qobuz".to_string()
    } else {
        "Unknown".to_string()
    }
}

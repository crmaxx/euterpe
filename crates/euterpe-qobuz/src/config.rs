use std::env;
use std::time::Duration;

use crate::error::QobuzError;
use crate::signing::FavoritesSignMode;

pub const DEFAULT_API_BASE: &str = "https://www.qobuz.com/api.json/0.2";
pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:83.0) Gecko/20100101 Firefox/83.0";
pub const DEFAULT_PLAY_BASE: &str = "https://play.qobuz.com";

#[derive(Debug, Clone)]
pub struct QobuzConfig {
    pub auth: AuthConfig,
    pub app_id: Option<String>,
    pub secrets: Option<Vec<String>>,
    pub api_base: String,
    pub play_base: String,
    pub user_agent: String,
    pub favorites_sign_mode: FavoritesSignMode,
    pub request_timeout: Duration,
    /// If true, call `user/login` after applying SessionToken (streamrip style).
    pub refresh_session_via_login: bool,
}

#[derive(Debug, Clone)]
pub enum AuthConfig {
    /// Default 2026: UAT from browser; no password login.
    SessionToken {
        user_id: u64,
        user_auth_token: String,
    },
    /// Optional: validate/refresh via `user/login`.
    TokenLogin {
        user_id: u64,
        user_auth_token: String,
    },
    /// Deprecated: expect 401 from API.
    EmailPassword { email: String, password: String },
}

impl Default for QobuzConfig {
    fn default() -> Self {
        Self {
            auth: AuthConfig::SessionToken {
                user_id: 0,
                user_auth_token: String::new(),
            },
            app_id: None,
            secrets: None,
            api_base: DEFAULT_API_BASE.to_string(),
            play_base: DEFAULT_PLAY_BASE.to_string(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            favorites_sign_mode: FavoritesSignMode::TimestampOnly,
            request_timeout: Duration::from_secs(30),
            refresh_session_via_login: false,
        }
    }
}

impl QobuzConfig {
    pub fn session_token(user_id: u64, user_auth_token: impl Into<String>) -> Self {
        Self {
            auth: AuthConfig::SessionToken {
                user_id,
                user_auth_token: user_auth_token.into(),
            },
            ..Default::default()
        }
    }

    pub fn from_env() -> Result<Self, QobuzError> {
        let user_id = env::var("EUTERPE_QOBUZ_USER_ID")
            .map_err(|_| QobuzError::Config("EUTERPE_QOBUZ_USER_ID is required".into()))?
            .parse::<u64>()
            .map_err(|e| QobuzError::Config(format!("invalid EUTERPE_QOBUZ_USER_ID: {e}")))?;

        let user_auth_token = env::var("EUTERPE_QOBUZ_AUTH_TOKEN")
            .map_err(|_| QobuzError::Config("EUTERPE_QOBUZ_AUTH_TOKEN is required".into()))?;

        let mut config = Self::session_token(user_id, user_auth_token);

        if let Ok(app_id) = env::var("EUTERPE_QOBUZ_APP_ID") {
            config.app_id = Some(app_id);
        }

        if let Ok(secrets_json) = env::var("EUTERPE_QOBUZ_SECRETS") {
            let secrets: Vec<String> = serde_json::from_str(&secrets_json).map_err(|e| {
                QobuzError::Config(format!("invalid EUTERPE_QOBUZ_SECRETS JSON: {e}"))
            })?;
            config.secrets = Some(secrets);
        }

        if let Ok(base) = env::var("EUTERPE_QOBUZ_API_BASE") {
            config.api_base = base;
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_requires_user_id_and_token() {
        std::env::remove_var("EUTERPE_QOBUZ_USER_ID");
        std::env::remove_var("EUTERPE_QOBUZ_AUTH_TOKEN");
        assert!(QobuzConfig::from_env().is_err());
    }
}

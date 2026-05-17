use crate::client::QobuzClient;
use crate::config::AuthConfig;
use crate::error::QobuzError;
use crate::models::{LoginResponse, UserProfile};

impl QobuzClient {
    pub async fn login(&mut self) -> Result<UserProfile, QobuzError> {
        match &self.config.auth {
            AuthConfig::SessionToken { .. } if !self.config.refresh_session_via_login => {
                return Err(QobuzError::Config(
                    "login() is a no-op for SessionToken unless refresh_session_via_login is set"
                        .into(),
                ));
            }
            _ => {}
        }

        let mut params = vec![("app_id", self.state.app_id.clone())];

        match &self.config.auth {
            AuthConfig::SessionToken {
                user_id,
                user_auth_token,
            }
            | AuthConfig::TokenLogin {
                user_id,
                user_auth_token,
            } => {
                params.push(("user_id", user_id.to_string()));
                params.push(("user_auth_token", user_auth_token.clone()));
            }
            AuthConfig::EmailPassword { email, password } => {
                params.push(("email", email.clone()));
                params.push(("password", password.clone()));
            }
        }

        let (status, body) = self.get_json("user/login", &params).await?;
        if status == 401 {
            return Err(QobuzError::Authentication(
                "invalid credentials or expired token; use EUTERPE_QOBUZ_AUTH_TOKEN".into(),
            ));
        }
        if status == 400 {
            return Err(QobuzError::InvalidAppId);
        }
        if status != 200 {
            return Err(QobuzError::from_status("user/login", status, &body));
        }

        let empty_params = body
            .pointer("/user/credential/parameters")
            .map(|v| v.is_object() && v.as_object().is_some_and(|o| o.is_empty()))
            .unwrap_or(true);
        if empty_params {
            return Err(QobuzError::Ineligible);
        }

        let login: LoginResponse = serde_json::from_value(body)?;
        self.state.user_auth_token = Some(login.user_auth_token.clone());
        self.apply_auth_headers();

        Ok(login.into_profile())
    }
}

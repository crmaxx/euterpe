use reqwest::Client;
use tracing::instrument;

use crate::bundle::bootstrap_app_id_and_secrets;
use crate::config::{AuthConfig, QobuzConfig};
use crate::error::QobuzError;

pub struct QobuzClient {
    pub(crate) http: Client,
    pub(crate) config: QobuzConfig,
    pub(crate) state: ClientState,
}

pub(crate) struct ClientState {
    pub app_id: String,
    pub secrets: Vec<String>,
    pub active_secret: Option<String>,
    pub user_auth_token: Option<String>,
}

impl QobuzClient {
    #[instrument(skip(config), fields(qobuz.bootstrap = true))]
    pub async fn connect(config: QobuzConfig) -> Result<Self, QobuzError> {
        let http = Client::builder()
            .timeout(config.request_timeout)
            .build()?;

        let (app_id, secrets) = bootstrap_app_id_and_secrets(
            &config.play_base,
            &http,
            config.app_id.as_deref(),
            config.secrets.as_deref(),
        )
        .await?;

        let user_auth_token = match &config.auth {
            AuthConfig::SessionToken {
                user_auth_token, ..
            }
            | AuthConfig::TokenLogin {
                user_auth_token, ..
            } => Some(user_auth_token.clone()),
            AuthConfig::EmailPassword { .. } => None,
        };

        let mut client = Self {
            http,
            config,
            state: ClientState {
                app_id,
                secrets,
                active_secret: None,
                user_auth_token,
            },
        };

        client.apply_auth_headers();

        if client.config.refresh_session_via_login {
            client.login().await?;
        }

        // App secret probe is deferred until `track_stream_url` (favorites/catalog need UAT only).
        Ok(client)
    }

    pub fn is_authenticated(&self) -> bool {
        self.state.user_auth_token.is_some()
    }

    #[instrument(skip(self), fields(qobuz.verify_session = true))]
    pub async fn verify_session(&self) -> Result<(), QobuzError> {
        self.favorites_albums(crate::pagination::PageRequest {
            limit: 1,
            offset: 0,
        })
        .await?;
        Ok(())
    }

    pub(crate) fn apply_auth_headers(&mut self) {
        // Headers are applied per-request in get_json
        let _ = &mut self.http;
    }

    pub(crate) async fn get_json(
        &self,
        endpoint: &str,
        params: &[(&str, String)],
    ) -> Result<(u16, serde_json::Value), QobuzError> {
        let url = format!(
            "{}/{}",
            self.config.api_base.trim_end_matches('/'),
            endpoint.trim_start_matches('/')
        );

        let mut req = self.http.get(&url).header("User-Agent", &self.config.user_agent);
        req = req.header("X-App-Id", &self.state.app_id);
        req = req
            .header("Content-Type", "application/json;charset=UTF-8")
            .query(params);

        if let Some(uat) = &self.state.user_auth_token {
            req = req.header("X-User-Auth-Token", uat);
        }

        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!(
                endpoint,
                url = %url,
                params = %redact_qobuz_params(params),
                "qobuz api request"
            );
        }

        let response = req.send().await?;
        let status = response.status().as_u16();
        let body: serde_json::Value = response.json().await.unwrap_or(serde_json::json!({}));

        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!(
                endpoint,
                status,
                body = %truncate_json_for_log(&body),
                "qobuz api response"
            );
        }

        Ok((status, body))
    }
}

fn redact_qobuz_params(params: &[(&str, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| {
            if *k == "user_auth_token" || *k == "password" {
                format!("{k}=…")
            } else {
                format!("{k}={v}")
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn truncate_json_for_log(body: &serde_json::Value) -> String {
    let s = body.to_string();
    const MAX: usize = 512;
    if s.len() <= MAX {
        s
    } else {
        format!("{}…", &s[..MAX])
    }
}

impl QobuzClient {
    /// Build a client for tests with pre-set app id, secrets, and token (no network bootstrap).
    pub fn new_for_test(config: QobuzConfig, app_id: String, secrets: Vec<String>) -> Self {
        let http = Client::builder()
            .timeout(config.request_timeout)
            .build()
            .expect("reqwest client");

        let user_auth_token = match &config.auth {
            AuthConfig::SessionToken {
                user_auth_token, ..
            }
            | AuthConfig::TokenLogin {
                user_auth_token, ..
            } => Some(user_auth_token.clone()),
            AuthConfig::EmailPassword { .. } => None,
        };

        Self {
            http,
            config,
            state: ClientState {
                app_id,
                secrets: secrets.clone(),
                active_secret: secrets.first().cloned(),
                user_auth_token,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::QobuzConfig;

    #[tokio::test]
    async fn connect_session_token_sets_headers_without_login() {
        let mut server = mockito::Server::new_async().await;
        let mut cfg = QobuzConfig::session_token(42, "uat-test");
        cfg.app_id = Some("123456789".into());
        cfg.secrets = Some(vec!["secret".into()]);
        cfg.api_base = server.url() + "/api.json/0.2";

        let _probe = server
            .mock(
                "GET",
                mockito::Matcher::Regex(r".*/track/getFileUrl.*".into()),
            )
            .with_status(200)
            .with_body(r#"{"url":"https://example.com/track.flac","format_id":5}"#)
            .create_async()
            .await;

        let mut client = QobuzClient::connect(cfg).await.unwrap();
        assert!(client.is_authenticated());
        assert_eq!(client.state.app_id, "123456789");
        assert!(client.state.active_secret.is_none());
        client.ensure_active_secret().await.unwrap();
        assert!(client.state.active_secret.is_some());
    }

    #[tokio::test]
    async fn token_login_success() {
        let mut server = mockito::Server::new_async().await;
        let mut cfg = QobuzConfig::session_token(1, "old-token");
        cfg.auth = AuthConfig::TokenLogin {
            user_id: 1,
            user_auth_token: "old-token".into(),
        };
        cfg.refresh_session_via_login = true;
        cfg.app_id = Some("123456789".into());
        cfg.secrets = Some(vec!["secret".into()]);
        cfg.api_base = server.url() + "/api.json/0.2";
        cfg.play_base = server.url();

        let login_body = include_str!("../tests/fixtures/login_ok.json");
        let _login = server
            .mock("GET", mockito::Matcher::Regex(r".*/user/login.*".into()))
            .with_status(200)
            .with_body(login_body)
            .create_async()
            .await;

        let _probe = server
            .mock("GET", mockito::Matcher::Regex(r".*/track/getFileUrl.*".into()))
            .with_status(200)
            .with_body(r#"{"url":"https://example.com/track.flac","format_id":5}"#)
            .create_async()
            .await;

        let mut client = QobuzClient::new_for_test(
            cfg.clone(),
            "123456789".into(),
            vec!["secret".into()],
        );
        client.config.api_base = cfg.api_base;
        let profile = client.login().await.unwrap();
        assert_eq!(profile.id, 99);
        assert_eq!(client.state.user_auth_token.as_deref(), Some("new-uat-token"));
    }

    #[tokio::test]
    async fn token_login_401() {
        let mut server = mockito::Server::new_async().await;
        let mut cfg = QobuzConfig::session_token(1, "bad");
        cfg.auth = AuthConfig::TokenLogin {
            user_id: 1,
            user_auth_token: "bad".into(),
        };
        cfg.refresh_session_via_login = true;
        cfg.app_id = Some("123456789".into());
        cfg.secrets = Some(vec!["secret".into()]);
        cfg.api_base = server.url() + "/api.json/0.2";

        let _login = server
            .mock("GET", mockito::Matcher::Regex(r".*/user/login.*".into()))
            .with_status(401)
            .with_body(r#"{"message":"Invalid credentials"}"#)
            .create_async()
            .await;

        let mut client = QobuzClient::new_for_test(cfg.clone(), "123456789".into(), vec!["s".into()]);
        client.config.api_base = cfg.api_base;
        let err = client.login().await.unwrap_err();
        assert!(matches!(err, QobuzError::Authentication(_)));
    }

    #[tokio::test]
    async fn token_login_ineligible() {
        let mut server = mockito::Server::new_async().await;
        let mut cfg = QobuzConfig::session_token(1, "tok");
        cfg.auth = AuthConfig::TokenLogin {
            user_id: 1,
            user_auth_token: "tok".into(),
        };
        cfg.refresh_session_via_login = true;
        cfg.api_base = server.url() + "/api.json/0.2";

        let _login = server
            .mock("GET", mockito::Matcher::Regex(r".*/user/login.*".into()))
            .with_status(200)
            .with_body(include_str!("../tests/fixtures/login_free.json"))
            .create_async()
            .await;

        let mut client = QobuzClient::new_for_test(cfg.clone(), "123456789".into(), vec!["s".into()]);
        client.config.api_base = cfg.api_base;
        let err = client.login().await.unwrap_err();
        assert!(matches!(err, QobuzError::Ineligible));
    }

    #[tokio::test]
    async fn verify_session_hits_favorites() {
        let mut server = mockito::Server::new_async().await;
        let mut cfg = QobuzConfig::session_token(1, "uat");
        cfg.app_id = Some("123456789".into());
        cfg.secrets = Some(vec!["secret".into()]);
        cfg.api_base = server.url() + "/api.json/0.2";

        let fav_body = include_str!("../tests/fixtures/favorites_albums_page0.json");
        let _fav = server
            .mock("GET", mockito::Matcher::Regex(r".*/favorite/getUserFavorites.*".into()))
            .with_status(200)
            .with_body(fav_body)
            .create_async()
            .await;

        let client = QobuzClient::new_for_test(cfg.clone(), "123456789".into(), vec!["secret".into()]);
        let mut client = client;
        client.config.api_base = cfg.api_base;
        client.verify_session().await.unwrap();
    }
}

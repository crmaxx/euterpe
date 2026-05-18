//! Browser OAuth flow (qobuz-dl PR #331 / paulborile `bug/newauth` branch).
//!
//! Authorize: `https://www.qobuz.com/signin/oauth?ext_app_id={app_id}&redirect_url={redirect_uri}`
//! Token exchange: `GET {api_base}/oauth/callback?code=&private_key=&app_id=`
//! Profile: `POST {api_base}/user/login` with `X-User-Auth-Token` and body `extra=partner`.

use reqwest::Client;
use serde::Deserialize;

use crate::bundle::{fetch_bundle, parse_app_id_from_bundle, parse_private_key_from_bundle};
use crate::config::{DEFAULT_API_BASE, DEFAULT_PLAY_BASE, DEFAULT_USER_AGENT};
use crate::error::QobuzError;

const OAUTH_SIGNIN_BASE: &str = "https://www.qobuz.com/signin/oauth";

#[derive(Debug, Clone)]
pub struct OAuthBootstrap {
    pub app_id: String,
    pub private_key: String,
}

#[derive(Debug, Clone)]
pub struct OAuthLoginResult {
    pub user_id: u64,
    pub user_auth_token: String,
    pub display_name: Option<String>,
    pub membership_label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthCallbackResponse {
    token: Option<String>,
}

/// Load `app_id` and OAuth `private_key` from play.qobuz.com bundle.js.
pub async fn fetch_oauth_bootstrap(
    play_base: &str,
    http: &Client,
) -> Result<OAuthBootstrap, QobuzError> {
    let bundle = fetch_bundle(play_base, http).await?;
    let app_id = parse_app_id_from_bundle(&bundle)?;
    let private_key = parse_private_key_from_bundle(&bundle).ok_or_else(|| {
        QobuzError::BundleParse(
            "OAuth private_key not found in bundle (update play.qobuz.com bundle?)".into(),
        )
    })?;
    Ok(OAuthBootstrap {
        app_id,
        private_key,
    })
}

/// Build the URL to open in the user's browser (Qobuz sign-in).
pub fn authorize_url(app_id: &str, redirect_uri: &str) -> String {
    format!(
        "{OAUTH_SIGNIN_BASE}?ext_app_id={}&redirect_url={}",
        encode_query_value(app_id),
        encode_query_value(redirect_uri),
    )
}

/// OAuth callback URL with CSRF `state` embedded (Qobuz does not echo a separate `state` param).
pub fn redirect_uri_with_state(callback_base: &str, state: &str) -> String {
    let base = callback_base.trim_end_matches('/');
    format!("{base}?state={}", encode_query_value(state))
}

pub fn encode_query_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

/// Exchange authorization `code` for UAT and load user profile (partner login).
pub async fn login_with_oauth_code(
    http: &Client,
    api_base: &str,
    app_id: &str,
    private_key: &str,
    code: &str,
) -> Result<OAuthLoginResult, QobuzError> {
    let base = api_base.trim_end_matches('/');
    let callback_url = format!("{base}/oauth/callback");
    let (status, body) = {
        let resp = http
            .get(&callback_url)
            .query(&[
                ("code", code),
                ("private_key", private_key),
                ("app_id", app_id),
            ])
            .header("User-Agent", DEFAULT_USER_AGENT)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
        (status, body)
    };

    if status == 403 || status == 401 {
        return Err(QobuzError::Authentication(
            "OAuth code exchange rejected (expired or invalid code)".into(),
        ));
    }
    if status >= 400 {
        return Err(QobuzError::Authentication(format!(
            "oauth/callback failed with status {status}"
        )));
    }

    let parsed: OAuthCallbackResponse = serde_json::from_value(body.clone()).map_err(|e| {
        QobuzError::Authentication(format!("invalid oauth/callback response: {e}"))
    })?;
    let token = parsed.token.filter(|t| !t.is_empty()).ok_or_else(|| {
        QobuzError::Authentication("oauth/callback response missing token".into())
    })?;

    let login_url = format!("{base}/user/login");
    let resp = http
        .post(&login_url)
        .query(&[("app_id", app_id)])
        .header("User-Agent", DEFAULT_USER_AGENT)
        .header("X-App-Id", app_id)
        .header("X-User-Auth-Token", &token)
        .header("Content-Type", "text/plain;charset=UTF-8")
        .body("extra=partner")
        .send()
        .await?;

    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
    if status == 401 {
        return Err(QobuzError::Authentication(
            "OAuth token rejected by user/login".into(),
        ));
    }
    if status >= 400 {
        return Err(QobuzError::Authentication(format!(
            "user/login after OAuth failed with status {status}"
        )));
    }

    let user_id = body
        .get("user")
        .and_then(|u| u.get("id"))
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)))
        .ok_or_else(|| QobuzError::Authentication("user/login missing user.id".into()))?;

    let user_auth_token = body
        .get("user_auth_token")
        .and_then(|v| v.as_str())
        .unwrap_or(&token)
        .to_string();

    let display_name = body
        .get("user")
        .and_then(|u| {
            u.get("display_name")
                .or_else(|| u.get("login"))
                .and_then(|v| v.as_str())
        })
        .map(str::to_string);

    let membership_label = body
        .get("user")
        .and_then(|u| u.get("credential"))
        .and_then(|c| c.get("parameters"))
        .and_then(|p| p.get("short_label"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Ok(OAuthLoginResult {
        user_id,
        user_auth_token,
        display_name,
        membership_label,
    })
}

pub fn default_api_base() -> &'static str {
    DEFAULT_API_BASE
}

pub fn default_play_base() -> &'static str {
    DEFAULT_PLAY_BASE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_encodes_redirect() {
        let redirect = redirect_uri_with_state(
            "http://127.0.0.1:8080/api/v1/qobuz/oauth/callback",
            "abc123",
        );
        let url = authorize_url("798273057", &redirect);
        assert!(url.starts_with("https://www.qobuz.com/signin/oauth?"));
        assert!(url.contains("ext_app_id=798273057"));
        assert!(url.contains("redirect_url=http%3A%2F%2F127.0.0.1"));
        assert!(url.contains("state%3Dabc123"));
    }
}

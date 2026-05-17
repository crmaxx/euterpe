use std::collections::BTreeMap;

use base64::Engine;
use regex::Regex;
use reqwest::Client;

use crate::config::DEFAULT_PLAY_BASE;
use crate::error::QobuzError;

const BUNDLE_URL_RE: &str =
    r#"<script src="(/resources/\d+\.\d+\.\d+-[a-z]\d{3}/bundle\.js)"></script>"#;
const APP_ID_RE: &str = r#"production:\{api:\{appId:"(?P<app_id>\d{9})""#;
const SEED_TZ_RE: &str =
    r#"[a-z]\.initialSeed\("(?P<seed>[\w=]+)",window\.utimezone\.(?P<timezone>[a-z]+)\)"#;

/// Parse `app_id` from bundle.js text (streamrip / qobuz-dl).
pub fn parse_app_id_from_bundle(bundle: &str) -> Result<String, QobuzError> {
    let re = Regex::new(APP_ID_RE).expect("valid app_id regex");
    re.captures(bundle)
        .and_then(|c| c.name("app_id").map(|m| m.as_str().to_string()))
        .ok_or_else(|| QobuzError::BundleParse("app_id not found in bundle".into()))
}

fn capitalize_timezone(tz: &str) -> String {
    let mut chars = tz.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

/// Decode app secrets from bundle.js (streamrip `QobuzSpoofer` / qobuz-dl `Bundle.get_secrets`).
pub fn decode_secrets(bundle: &str) -> Result<Vec<String>, QobuzError> {
    let seed_re = Regex::new(SEED_TZ_RE).expect("valid seed regex");
    let mut secrets: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for cap in seed_re.captures_iter(bundle) {
        let seed = cap.name("seed").unwrap().as_str();
        let timezone = cap.name("timezone").unwrap().as_str();
        secrets
            .entry(timezone.to_string())
            .or_default()
            .push(seed.to_string());
    }

    if secrets.len() < 2 {
        return Err(QobuzError::BundleParse(
            "expected at least two timezone seeds".into(),
        ));
    }

    // streamrip / qobuz-dl: prioritize the *second* captured timezone block.
    let mut keys: Vec<String> = secrets.keys().cloned().collect();
    let second = keys.remove(1);
    keys.insert(0, second);

    let timezones_pattern = keys
        .iter()
        .map(|tz| capitalize_timezone(tz))
        .collect::<Vec<_>>()
        .join("|");

    let info_re = Regex::new(&format!(
        r#"name:"\w+/({timezones_pattern})",info:"(?P<info>[\w=]+)",extras:"(?P<extras>[\w=]+)""#
    ))
    .map_err(|e| QobuzError::BundleParse(e.to_string()))?;

    for cap in info_re.captures_iter(bundle) {
        let mut tz = cap.get(1).unwrap().as_str().to_lowercase();
        if tz == "algiers" {
            tz = "algier".to_string();
        }
        let info = cap.name("info").unwrap().as_str();
        let extras = cap.name("extras").unwrap().as_str();
        if let Some(entry) = secrets.get_mut(&tz) {
            entry.push(info.to_string());
            entry.push(extras.to_string());
        }
    }

    let mut decoded = Vec::new();
    for key in &keys {
        let parts = secrets.get(key).ok_or_else(|| {
            QobuzError::BundleParse(format!("missing info/extras for timezone {key}"))
        })?;
        let joined = parts.join("");
        if joined.len() <= 44 {
            return Err(QobuzError::BundleParse(format!(
                "joined secret payload too short for {key}"
            )));
        }
        let b64 = &joined[..joined.len() - 44];
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| QobuzError::BundleParse(format!("base64 decode failed: {e}")))?;
        let secret = String::from_utf8(bytes)
            .map_err(|e| QobuzError::BundleParse(format!("utf8 decode failed: {e}")))?;
        if !secret.is_empty() {
            decoded.push(secret);
        }
    }

    if decoded.is_empty() {
        return Err(QobuzError::BundleParse("no secrets decoded".into()));
    }

    Ok(decoded)
}

/// Fetch login page and bundle.js from play.qobuz.com.
pub async fn fetch_bundle(play_base: &str, http: &Client) -> Result<String, QobuzError> {
    let login_url = format!("{play_base}/login");
    let login_html = http
        .get(&login_url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| QobuzError::BundleParse(format!("login page fetch failed: {e}")))?
        .text()
        .await?;

    let bundle_re = Regex::new(BUNDLE_URL_RE).expect("valid bundle url regex");
    let path = bundle_re
        .captures(&login_html)
        .and_then(|c| c.get(1).map(|m| m.as_str()))
        .ok_or_else(|| QobuzError::BundleParse("bundle.js URL not found on login page".into()))?;

    let bundle_url = format!("{play_base}{path}");
    let bundle = http
        .get(&bundle_url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| QobuzError::BundleParse(format!("bundle.js fetch failed: {e}")))?
        .text()
        .await?;

    Ok(bundle)
}

pub async fn bootstrap_app_id_and_secrets(
    play_base: &str,
    http: &Client,
    app_id_override: Option<&str>,
    secrets_override: Option<&[String]>,
) -> Result<(String, Vec<String>), QobuzError> {
    if let (Some(app_id), Some(secrets)) = (app_id_override, secrets_override) {
        return Ok((app_id.to_string(), secrets.to_vec()));
    }

    let bundle = fetch_bundle(play_base, http).await?;
    let app_id = app_id_override
        .map(str::to_string)
        .unwrap_or_else(|| parse_app_id_from_bundle(&bundle).unwrap_or_default());
    let secrets = if let Some(sec) = secrets_override {
        sec.to_vec()
    } else {
        decode_secrets(&bundle)?
    };

    Ok((app_id, secrets))
}

pub fn default_play_base() -> &'static str {
    DEFAULT_PLAY_BASE
}

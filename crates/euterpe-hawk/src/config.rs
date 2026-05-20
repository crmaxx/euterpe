use std::env;
use std::time::Duration;

use crate::token::collector_endpoint_from_token;

#[derive(Debug, Clone)]
pub struct HawkConfig {
    pub token: String,
    pub collector_endpoint: String,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub context: Option<serde_json::Value>,
    pub source_code_enabled: bool,
    pub source_code_lines: usize,
    pub backtrace_trim: bool,
    pub default_user: Option<super::event::AffectedUser>,
    pub batch_max: usize,
    pub batch_interval: Duration,
    pub sample_rate: f32,
    pub flush_timeout: Duration,
    pub dedup_window: Duration,
}

impl HawkConfig {
    pub fn from_token(token: impl Into<String>) -> Result<Self, super::token::InvalidHawkToken> {
        let token = token.into();
        let collector_endpoint = collector_endpoint_from_token(&token)?;
        Ok(Self {
            token,
            collector_endpoint,
            release: None,
            environment: None,
            context: None,
            source_code_enabled: default_source_code_enabled(),
            source_code_lines: 5,
            backtrace_trim: true,
            default_user: None,
            batch_max: 1,
            batch_interval: Duration::from_millis(1000),
            sample_rate: 1.0,
            flush_timeout: Duration::from_secs(2),
            dedup_window: Duration::from_secs(5),
        })
    }

    pub fn from_env() -> Option<Self> {
        let token = env::var("HAWK_TOKEN").ok().filter(|t| !t.is_empty())?;
        let mut config = match Self::from_token(token) {
            Ok(c) => c,
            Err(_) => return None,
        };
        if let Ok(endpoint) = env::var("HAWK_COLLECTOR_ENDPOINT") {
            if !endpoint.is_empty() {
                config.collector_endpoint = endpoint.trim_end_matches('/').to_string();
            }
        }
        config.release = env::var("HAWK_RELEASE")
            .ok()
            .filter(|s| !s.is_empty());
        config.environment = env::var("HAWK_ENVIRONMENT")
            .ok()
            .filter(|s| !s.is_empty());
        config.backtrace_trim = env_bool("HAWK_BACKTRACE_TRIM", true);
        config.batch_max = env_usize("HAWK_BATCH_MAX", 1).max(1);
        config.batch_interval =
            Duration::from_millis(env_u64("HAWK_BATCH_INTERVAL_MS", 1000).max(1));
        config.sample_rate = env_f32("HAWK_SAMPLE_RATE", 1.0).clamp(0.0, 1.0);
        config.flush_timeout =
            Duration::from_secs(env_u64("HAWK_FLUSH_TIMEOUT_SECS", 2).max(1));
        config.dedup_window =
            Duration::from_secs(env_u64("HAWK_DEDUP_WINDOW_SECS", 5).max(1));
        Some(config)
    }
}

fn default_source_code_enabled() -> bool {
    env::var("RUST_BACKTRACE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("full"))
        .unwrap_or(false)
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_f32(key: &str, default: f32) -> f32 {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

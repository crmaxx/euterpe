use thiserror::Error;

#[derive(Debug, Error)]
pub enum QobuzError {
    #[error("authentication failed: {0}")]
    Authentication(String),

    #[error("account is not eligible for streaming (free tier)")]
    Ineligible,

    #[error("invalid app id")]
    InvalidAppId,

    #[error("invalid app secret")]
    InvalidAppSecret,

    #[error("invalid request signature")]
    InvalidSignature,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("forbidden")]
    Forbidden,

    #[error("not found: {endpoint}: {message}")]
    NotFound { endpoint: String, message: String },

    #[error("rate limited")]
    RateLimit,

    #[error("track is not streamable: {0}")]
    NonStreamable(String),

    #[error("invalid quality format_id")]
    InvalidQuality,

    #[error("upstream error ({status}): {message}")]
    Upstream { status: u16, message: String },

    #[error("bundle parse error: {0}")]
    BundleParse(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl QobuzError {
    pub fn from_status(endpoint: &str, status: u16, body: &serde_json::Value) -> Self {
        let message = body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error")
            .to_string();

        match status {
            400 => {
                let lower = message.to_lowercase();
                if lower.contains("secret") || lower.contains("sig") {
                    Self::InvalidSignature
                } else if lower.contains("app") {
                    Self::InvalidAppId
                } else {
                    Self::BadRequest(message)
                }
            }
            401 => Self::Authentication(format!(
                "{message}. Use EUTERPE_QOBUZ_USER_ID and EUTERPE_QOBUZ_AUTH_TOKEN (password login is deprecated)."
            )),
            403 => Self::Forbidden,
            404 => Self::NotFound {
                endpoint: endpoint.to_string(),
                message,
            },
            429 => Self::RateLimit,
            500..=599 => Self::Upstream { status, message },
            _ => Self::Upstream { status, message },
        }
    }
}

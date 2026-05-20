use std::sync::Arc;

use serde_json::Value;

use crate::crypto::MasterKey;
use crate::db::integrations::IntegrationRow;
use crate::error::ApiError;
use crate::integrations::catalog::{IntegrationProvider, IntegrationType};
use crate::integrations::discogs::DiscogsProvider;
use crate::integrations::gnudb::GnudbProvider;
use crate::integrations::musicbrainz::MusicBrainzProvider;
use crate::integrations::provider::TagSourceProvider;
use crate::integrations::tracktype::TrackTypeProvider;

pub fn build_tag_source(
    row: &IntegrationRow,
    master_key: Option<&MasterKey>,
) -> Result<Arc<dyn TagSourceProvider>, ApiError> {
    if IntegrationType::parse(&row.type_) != Some(IntegrationType::TagSource) {
        return Err(ApiError::bad_request("invalid integration type"));
    }
    let provider = IntegrationProvider::parse(&row.provider)
        .ok_or_else(|| ApiError::bad_request("unknown provider"))?;
    let config: Value = serde_json::from_str(&row.config_json)
        .map_err(|e| ApiError::bad_request(format!("invalid config_json: {e}")))?;

    match provider {
        IntegrationProvider::MusicBrainz => {
            let contact = config.get("contact").and_then(|v| v.as_str()).unwrap_or("");
            Ok(Arc::new(MusicBrainzProvider::new(contact)?))
        }
        IntegrationProvider::Discogs => {
            let token = decrypt_secret(row, master_key, "token")?;
            Ok(Arc::new(DiscogsProvider::new(&token)?))
        }
        IntegrationProvider::Gnudb => {
            let server = config.get("server_base").and_then(|v| v.as_str());
            Ok(Arc::new(GnudbProvider::new(server)?))
        }
        IntegrationProvider::Tracktype => {
            let api_base = config.get("api_base").and_then(|v| v.as_str());
            let api_key = config.get("api_key").and_then(|v| v.as_str());
            let key_from_secret = row
                .config_secrets_enc
                .as_ref()
                .and_then(|_| decrypt_secret(row, master_key, "api_key").ok());
            Ok(Arc::new(TrackTypeProvider::new(
                api_base,
                api_key.or(key_from_secret.as_deref()),
            )?))
        }
    }
}

fn decrypt_secret(
    row: &IntegrationRow,
    master_key: Option<&MasterKey>,
    field: &str,
) -> Result<String, ApiError> {
    let enc = row
        .config_secrets_enc
        .as_deref()
        .ok_or_else(|| ApiError::bad_request(format!("missing secret for {field}")))?;
    let master = master_key.ok_or_else(|| {
        ApiError::Message("EUTERPE_MASTER_KEY is required for this integration".into())
    })?;
    let json: Value = serde_json::from_str(&master.decrypt(enc)?)
        .map_err(|e| ApiError::Message(format!("invalid secrets blob: {e}")))?;
    json.get(field)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| ApiError::bad_request(format!("secret field {field} missing")))
}

pub fn validate_config(
    provider: IntegrationProvider,
    config: &Value,
    secrets: Option<&Value>,
    master_key: Option<&MasterKey>,
) -> Result<(), ApiError> {
    match provider {
        IntegrationProvider::MusicBrainz => {
            let contact = config.get("contact").and_then(|v| v.as_str()).unwrap_or("");
            MusicBrainzProvider::new(contact)?;
        }
        IntegrationProvider::Discogs => {
            let token = secrets
                .and_then(|s| s.get("token"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ApiError::bad_request("Discogs token is required"))?;
            if master_key.is_none() {
                return Err(ApiError::Message(
                    "EUTERPE_MASTER_KEY is required for Discogs".into(),
                ));
            }
            DiscogsProvider::new(token)?;
        }
        IntegrationProvider::Gnudb => {
            let server = config.get("server_base").and_then(|v| v.as_str());
            GnudbProvider::new(server)?;
        }
        IntegrationProvider::Tracktype => {
            let api_base = config.get("api_base").and_then(|v| v.as_str());
            let api_key = secrets
                .and_then(|s| s.get("api_key"))
                .and_then(|v| v.as_str())
                .or_else(|| config.get("api_key").and_then(|v| v.as_str()));
            TrackTypeProvider::new(api_base, api_key)?;
        }
    }
    Ok(())
}

pub fn encrypt_secrets(master_key: &MasterKey, secrets: &Value) -> Result<String, ApiError> {
    let s = serde_json::to_string(secrets).map_err(|e| ApiError::Message(e.to_string()))?;
    master_key.encrypt(&s)
}

use base64::Engine;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid hawk integration token")]
pub struct InvalidHawkToken;

#[derive(Debug, Deserialize)]
struct TokenPayload {
    #[serde(rename = "integrationId")]
    integration_id: String,
}

/// Decode integration token and build default collector URL (`https://{id}.k1.hawk.so`).
pub fn collector_endpoint_from_token(token: &str) -> Result<String, InvalidHawkToken> {
    if token.is_empty() {
        return Err(InvalidHawkToken);
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(token)
        .map_err(|_| InvalidHawkToken)?;
    let payload: TokenPayload = serde_json::from_slice(&decoded).map_err(|_| InvalidHawkToken)?;
    if payload.integration_id.is_empty() {
        return Err(InvalidHawkToken);
    }
    Ok(format!("https://{}.k1.hawk.so", payload.integration_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TOKEN: &str = "eyJpbnRlZ3JhdGlvbklkIjoiZGRjZmY4OTItODMzMy00YjVlLWIyYWQtZWM1MDQ5MDVjMjFlIiwic2VjcmV0IjoiZmJjYzIwMTEtMTY5My00NDIyLThiNDItZDRlMzdlYmI4NWIwIn0=";

    #[test]
    fn parses_sample_token() {
        let url = collector_endpoint_from_token(SAMPLE_TOKEN).unwrap();
        assert_eq!(
            url,
            "https://ddcff892-8333-4b5e-b2ad-ec504905c21e.k1.hawk.so"
        );
    }

    #[test]
    fn rejects_empty_token() {
        assert!(collector_endpoint_from_token("").is_err());
    }

    #[test]
    fn rejects_invalid_base64_json() {
        assert!(collector_endpoint_from_token("not-valid!!!").is_err());
    }

    #[test]
    fn rejects_token_without_integration_id() {
        let token = base64::engine::general_purpose::STANDARD.encode(r#"{"secret":"x"}"#);
        assert!(collector_endpoint_from_token(&token).is_err());
    }
}

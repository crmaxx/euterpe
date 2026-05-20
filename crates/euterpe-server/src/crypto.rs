use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{Engine, engine::general_purpose::STANDARD as B64};

use crate::error::ApiError;

const NONCE_LEN: usize = 12;

#[derive(Clone, Debug)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    pub fn parse(s: &str) -> Result<Self, ApiError> {
        let bytes = if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
            hex::decode(s)
                .map_err(|e| ApiError::Config(format!("invalid EUTERPE_MASTER_KEY hex: {e}")))?
        } else {
            B64.decode(s)
                .map_err(|e| ApiError::Config(format!("invalid EUTERPE_MASTER_KEY base64: {e}")))?
        };
        if bytes.len() != 32 {
            return Err(ApiError::Config(
                "EUTERPE_MASTER_KEY must be 32 bytes".into(),
            ));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(Self(key))
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, ApiError> {
        let cipher = Aes256Gcm::new_from_slice(&self.0)
            .map_err(|e| ApiError::Config(format!("cipher init: {e}")))?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| ApiError::Message(format!("encrypt failed: {e}")))?;
        let mut out = nonce.to_vec();
        out.extend(ciphertext);
        Ok(B64.encode(out))
    }

    pub fn decrypt(&self, encoded: &str) -> Result<String, ApiError> {
        let data = B64
            .decode(encoded)
            .map_err(|e| ApiError::Message(format!("invalid ciphertext: {e}")))?;
        if data.len() <= NONCE_LEN {
            return Err(ApiError::Message("ciphertext too short".into()));
        }
        let (nonce_bytes, ct) = data.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let cipher = Aes256Gcm::new_from_slice(&self.0)
            .map_err(|e| ApiError::Config(format!("cipher init: {e}")))?;
        let plain = cipher
            .decrypt(nonce, ct)
            .map_err(|e| ApiError::Message(format!("decrypt failed: {e}")))?;
        String::from_utf8(plain).map_err(|e| ApiError::Message(format!("utf8: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_hex_key() {
        let key = MasterKey::parse(&hex::encode([7u8; 32])).unwrap();
        let enc = key.encrypt("secret-token").unwrap();
        assert_ne!(enc, "secret-token");
        assert_eq!(key.decrypt(&enc).unwrap(), "secret-token");
    }
}

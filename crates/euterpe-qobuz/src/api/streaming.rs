use crate::client::QobuzClient;
use crate::error::QobuzError;
use crate::models::StreamUrl;
use crate::signing::sign_track_file_url;

/// Qobuz `format_id` values for `track/getFileUrl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Mp3_320,
    FlacCd,
    FlacHiRes,
    FlacHiResPlus,
}

impl Quality {
    pub fn format_id(self) -> u8 {
        match self {
            Self::Mp3_320 => 5,
            Self::FlacCd => 6,
            Self::FlacHiRes => 7,
            Self::FlacHiResPlus => 27,
        }
    }

    pub fn from_streamrip_level(level: u8) -> Option<Self> {
        match level {
            1 => Some(Self::Mp3_320),
            2 => Some(Self::FlacCd),
            3 => Some(Self::FlacHiRes),
            4 => Some(Self::FlacHiResPlus),
            _ => None,
        }
    }
}

/// Probe track IDs used by reference clients (streamrip / qobuz-sync).
const PROBE_TRACK_IDS: &[u64] = &[19_512_574, 5_966_783];

impl QobuzClient {
    pub async fn track_stream_url(
        &mut self,
        track_id: u64,
        quality: Quality,
    ) -> Result<StreamUrl, QobuzError> {
        self.ensure_active_secret().await?;
        let secret = self
            .state
            .active_secret
            .as_deref()
            .ok_or(QobuzError::InvalidAppSecret)?;
        self.track_stream_url_with_secret(track_id, quality, secret)
            .await
    }

    /// Select a working app secret (streamrip `_get_valid_secret`). Called lazily before streaming.
    pub async fn ensure_active_secret(&mut self) -> Result<(), QobuzError> {
        if self.state.active_secret.is_some() {
            return Ok(());
        }
        for secret in self.state.secrets.clone() {
            if self.probe_secret(&secret).await {
                self.state.active_secret = Some(secret);
                return Ok(());
            }
        }
        Err(QobuzError::InvalidAppSecret)
    }

    /// Returns true if `secret` is accepted for signing (streamrip: status 200 or 401, not 400).
    pub(crate) async fn probe_secret(&self, secret: &str) -> bool {
        for &track_id in PROBE_TRACK_IDS {
            match self.file_url_status(track_id, Quality::Mp3_320, secret).await {
                Ok(400) => continue,
                // 200 with restrictions / empty url still proves the secret signature is valid.
                Ok(200) | Ok(401) | Ok(403) => return true,
                Ok(_) => return true,
                Err(_) => continue,
            }
        }
        false
    }

    async fn file_url_status(
        &self,
        track_id: u64,
        quality: Quality,
        secret: &str,
    ) -> Result<u16, QobuzError> {
        let format_id = quality.format_id();
        let request_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let request_sig = sign_track_file_url(format_id, track_id, request_ts, secret);

        let params = vec![
            ("track_id", track_id.to_string()),
            ("format_id", format_id.to_string()),
            ("intent", "stream".to_string()),
            ("request_ts", request_ts.to_string()),
            ("request_sig", request_sig),
        ];

        let (status, _) = self.get_json("track/getFileUrl", &params).await?;
        Ok(status)
    }

    async fn track_stream_url_with_secret(
        &self,
        track_id: u64,
        quality: Quality,
        secret: &str,
    ) -> Result<StreamUrl, QobuzError> {
        let format_id = quality.format_id();
        if ![5, 6, 7, 27].contains(&format_id) {
            return Err(QobuzError::InvalidQuality);
        }

        let request_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let request_sig = sign_track_file_url(format_id, track_id, request_ts, secret);

        let params = vec![
            ("track_id", track_id.to_string()),
            ("format_id", format_id.to_string()),
            ("intent", "stream".to_string()),
            ("request_ts", request_ts.to_string()),
            ("request_sig", request_sig),
        ];

        let (status, body) = self.get_json("track/getFileUrl", &params).await?;
        if status == 400 {
            return Err(QobuzError::InvalidAppSecret);
        }
        if status != 200 {
            return Err(QobuzError::from_status("track/getFileUrl", status, &body));
        }

        let stream: StreamUrl = serde_json::from_value(body)?;
        if stream.url.is_none() {
            let msg = stream
                .restrictions
                .as_ref()
                .and_then(|r| r.first())
                .and_then(|x| x.code.clone())
                .unwrap_or_else(|| "not streamable".to_string());
            return Err(QobuzError::NonStreamable(msg));
        }

        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::Quality;

    #[test]
    fn quality_format_ids() {
        assert_eq!(Quality::Mp3_320.format_id(), 5);
        assert_eq!(Quality::FlacCd.format_id(), 6);
        assert_eq!(Quality::FlacHiRes.format_id(), 7);
        assert_eq!(Quality::FlacHiResPlus.format_id(), 27);
    }

    #[test]
    fn streamrip_level_mapping() {
        assert_eq!(Quality::from_streamrip_level(1), Some(Quality::Mp3_320));
        assert_eq!(Quality::from_streamrip_level(4), Some(Quality::FlacHiResPlus));
        assert!(Quality::from_streamrip_level(9).is_none());
    }
}

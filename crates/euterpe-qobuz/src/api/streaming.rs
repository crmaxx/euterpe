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

const PROBE_TRACK_ID: u64 = 5_966_783;

impl QobuzClient {
    pub async fn track_stream_url(
        &self,
        track_id: u64,
        quality: Quality,
    ) -> Result<StreamUrl, QobuzError> {
        let secret = self
            .state
            .active_secret
            .as_deref()
            .ok_or(QobuzError::InvalidAppSecret)?;
        self.track_stream_url_with_secret(track_id, quality, secret)
            .await
    }

    pub(crate) async fn probe_secret(&self, secret: &str) -> bool {
        match self
            .track_stream_url_with_secret(PROBE_TRACK_ID, Quality::Mp3_320, secret)
            .await
        {
            Ok(_) => true,
            Err(QobuzError::NonStreamable(_)) => true,
            Err(QobuzError::Authentication(_)) => true,
            Err(QobuzError::InvalidAppSecret) | Err(QobuzError::InvalidSignature) => false,
            Err(_) => false,
        }
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
            .as_secs() as i64;
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

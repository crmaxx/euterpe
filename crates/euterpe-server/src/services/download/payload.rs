use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DownloadJobPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_api_id: Option<String>,
}

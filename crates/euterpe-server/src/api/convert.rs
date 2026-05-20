use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertJobSummary {
    pub id: i64,
    pub album_id: i64,
    pub status: String,
    pub trigger: String,
    pub files_total: i64,
    pub files_done: i64,
    pub progress_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertJobResponse {
    pub job: ConvertJobSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertAlbumResponse {
    pub job_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertProgressEvent {
    pub job_id: i64,
    pub album_id: i64,
    pub status: String,
    pub files_total: i64,
    pub files_done: i64,
    pub progress_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

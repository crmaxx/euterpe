use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryScanStartResponse {
    pub scan_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryScanRunSummary {
    pub id: i64,
    pub status: String,
    /// Audio files discovered during enumerate (live while total unknown).
    pub files_seen: i64,
    /// Paths for which index job was queued (tags + hash done).
    pub files_processed: i64,
    pub files_indexed: i64,
    /// Set when enumerate finished; 0 while still discovering.
    pub files_total: i64,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryScanLatestResponse {
    pub run: Option<LibraryScanRunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgressEvent {
    pub scan_id: i64,
    pub files_seen: i64,
    pub files_processed: i64,
    pub files_indexed: i64,
    pub files_total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAlbumItem {
    pub id: i64,
    pub title: String,
    pub artist_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    pub track_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAlbumListResponse {
    pub items: Vec<LibraryAlbumItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryTrackItem {
    pub id: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_sec: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAlbumDetailResponse {
    pub id: i64,
    pub title: String,
    pub artist_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_total: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_total: Option<i32>,
    pub tracks: Vec<LibraryTrackItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAlbumTagsPatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_total: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_total: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryTrackDetailResponse {
    pub id: i64,
    pub album_id: i64,
    pub title: String,
    pub artist_name: String,
    pub album_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_sec: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumCoverUploadResponse {
    pub cover_path: String,
    pub tracks_embedded: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryTrackTagsPatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
}

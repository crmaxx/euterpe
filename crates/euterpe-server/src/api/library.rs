use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueIssue {
    pub code: String,
    pub message: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueExtraField {
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i32>,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueFileChoice {
    pub path: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueTrack {
    pub number: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub start_index: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pregap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueDocument {
    pub cue_path: String,
    pub audio_path: String,
    pub audio_format: String,
    pub album_title: String,
    pub album_artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default)]
    pub extra_fields: Vec<CueExtraField>,
    pub tracks: Vec<CueTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueValidationResponse {
    pub valid: bool,
    pub issues: Vec<CueIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueAlbumResponse {
    pub cue_files: Vec<CueFileChoice>,
    pub document: CueDocument,
    pub validation: CueValidationResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CueValidateRequest {
    pub document: CueDocument,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CueSplitRequest {
    pub document: CueDocument,
    pub source_file_policy: String,
    #[serde(default)]
    pub file_mask: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueSplitResponse {
    pub job_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueJobSummary {
    pub id: i64,
    pub album_id: i64,
    pub status: String,
    pub tracks_total: i64,
    pub tracks_done: i64,
    pub progress_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueJobResponse {
    pub job: Option<CueJobSummary>,
}

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
    pub has_cue_files: bool,
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
    pub has_convertible_tracks: bool,
    pub has_cue_files: bool,
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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumLookupTrack {
    pub title: String,
    pub track_number: Option<i32>,
    pub duration_sec: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct AlbumLookupContext {
    pub artist_name: String,
    pub album_title: String,
    pub year: Option<i32>,
    pub tracks: Vec<AlbumLookupTrack>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlbumLookupResult {
    pub candidates: Vec<MetadataCandidate>,
    pub page: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataCandidate {
    pub id: String,
    pub title: String,
    pub artist_name: String,
    pub year: Option<i32>,
    pub score: f32,
    pub track_count: Option<u32>,
    pub source_label: String,
}

#[derive(Debug, Clone)]
pub struct AlbumMetadataTrack {
    pub title: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlbumMetadataRelease {
    pub title: String,
    pub artist_name: String,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub tracks: Vec<AlbumMetadataTrack>,
    pub cover_url: Option<String>,
}

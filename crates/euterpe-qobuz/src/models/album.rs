use serde::Deserialize;

use super::artist::ArtistRef;
use super::track::TrackSummary;

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    pub small: Option<String>,
    pub thumbnail: Option<String>,
    pub large: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlbumSummary {
    pub id: u64,
    pub title: String,
    pub artist: Option<ArtistRef>,
    pub artists: Option<Vec<ArtistRef>>,
    pub image: Option<Image>,
    #[serde(rename = "release_date_original")]
    pub release_date_original: Option<String>,
    pub hires: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlbumDetail {
    #[serde(flatten)]
    pub summary: AlbumSummary,
    pub tracks: Option<AlbumTracks>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlbumTracks {
    pub items: Vec<TrackSummary>,
}

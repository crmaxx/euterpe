use serde::Deserialize;

use super::artist::ArtistRef;

#[derive(Debug, Clone, Deserialize)]
pub struct TrackSummary {
    pub id: u64,
    pub title: String,
    #[serde(rename = "track_number")]
    pub track_number: Option<u32>,
    pub duration: Option<u32>,
    pub performer: Option<ArtistRef>,
    #[serde(rename = "hires_streamable")]
    pub hires_streamable: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamUrl {
    pub url: Option<String>,
    #[serde(rename = "format_id")]
    pub format_id: Option<u8>,
    #[serde(rename = "sampling_rate")]
    pub sampling_rate: Option<u32>,
    #[serde(rename = "bit_depth")]
    pub bit_depth: Option<u32>,
    pub restrictions: Option<Vec<StreamRestriction>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamRestriction {
    pub code: Option<String>,
}

use serde::Deserialize;

use super::artist::ArtistRef;
use super::catalog_meta::GenreRef;
use super::deser::{deserialize_null_string, deserialize_opt_f64, deserialize_qobuz_id};

#[derive(Debug, Clone, Deserialize)]
pub struct TrackSummary {
    #[serde(deserialize_with = "deserialize_qobuz_id")]
    pub id: u64,
    #[serde(default, deserialize_with = "deserialize_null_string")]
    pub title: String,
    #[serde(rename = "track_number")]
    pub track_number: Option<u32>,
    pub duration: Option<u32>,
    pub performer: Option<ArtistRef>,
    #[serde(rename = "hires_streamable")]
    pub hires_streamable: Option<bool>,
    #[serde(rename = "media_number", default)]
    pub media_number: Option<u32>,
    #[serde(default)]
    pub genre: Option<GenreRef>,
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub composer: Option<ArtistRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamUrl {
    pub url: Option<String>,
    #[serde(rename = "format_id")]
    pub format_id: Option<u8>,
    #[serde(
        rename = "sampling_rate",
        deserialize_with = "deserialize_opt_f64",
        default
    )]
    pub sampling_rate: Option<f64>,
    #[serde(
        rename = "bit_depth",
        deserialize_with = "deserialize_opt_f64",
        default
    )]
    pub bit_depth: Option<f64>,
    pub restrictions: Option<Vec<StreamRestriction>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamRestriction {
    pub code: Option<String>,
}

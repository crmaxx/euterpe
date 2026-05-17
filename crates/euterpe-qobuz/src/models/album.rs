use serde::de::{self, Deserializer};
use serde::Deserialize;

use super::artist::ArtistRef;
use super::deser::{deserialize_null_string, parse_id_value};
use super::track::TrackSummary;

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    pub small: Option<String>,
    pub thumbnail: Option<String>,
    pub large: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlbumSummary {
    pub id: u64,
    pub title: String,
    pub artist: Option<ArtistRef>,
    pub artists: Option<Vec<ArtistRef>>,
    pub image: Option<Image>,
    pub release_date_original: Option<String>,
    pub hires: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AlbumSummaryRaw {
    #[serde(rename = "qobuz_id", default)]
    qobuz_id: Option<serde_json::Value>,
    id: serde_json::Value,
    #[serde(default, deserialize_with = "deserialize_null_string")]
    title: String,
    artist: Option<ArtistRef>,
    artists: Option<Vec<ArtistRef>>,
    image: Option<Image>,
    #[serde(rename = "release_date_original")]
    release_date_original: Option<String>,
    hires: Option<bool>,
}

impl<'de> Deserialize<'de> for AlbumSummary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = AlbumSummaryRaw::deserialize(deserializer)?;
        // API album_id is `qobuz_id`; `id` is often a UPC/barcode (see qobuz-sync / streamrip).
        let id = raw
            .qobuz_id
            .as_ref()
            .and_then(|v| parse_id_value(v).ok())
            .or_else(|| {
                parse_id_value(&raw.id).ok().filter(|&n| {
                    // Qobuz catalog ids are small; UPC strings parse to huge u64.
                    n < 100_000_000_000
                })
            })
            .ok_or_else(|| {
                de::Error::custom(format!(
                    "album missing qobuz_id (got id {:?}, qobuz_id {:?})",
                    raw.id, raw.qobuz_id
                ))
            })?;
        Ok(Self {
            id,
            title: raw.title,
            artist: raw.artist,
            artists: raw.artists,
            image: raw.image,
            release_date_original: raw.release_date_original,
            hires: raw.hires,
        })
    }
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

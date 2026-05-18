use serde::de::{self, Deserializer};
use serde::Deserialize;

use super::artist::ArtistRef;
use super::catalog_meta::{GenreRef, LabelRef};
use super::deser::{deserialize_null_string, parse_album_ref_value, parse_id_value};
use super::track::TrackSummary;

/// Numeric `id` from favorites without `qobuz_id` above this are treated as UPC-like, not catalog ids.
const MAX_FALLBACK_NUMERIC_ID: u64 = 100_000_000;

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    pub small: Option<String>,
    pub thumbnail: Option<String>,
    pub large: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlbumSummary {
    /// Catalog id (`qobuz_id`) for favorites API and download jobs.
    pub id: u64,
    /// `qobuz_id` field from JSON when present (may match `id`).
    pub qobuz_id: Option<u64>,
    pub title: String,
    pub artist: Option<ArtistRef>,
    pub artists: Option<Vec<ArtistRef>>,
    pub image: Option<Image>,
    pub release_date_original: Option<String>,
    pub hires: Option<bool>,
    /// Short opaque id for `album/get` (`id` field on album/get, e.g. `zg7pv28g4mldg`).
    pub album_ref: Option<String>,
    /// Long URL slug (human-readable); also accepted by `album/get` when `album_ref` is absent.
    pub slug: Option<String>,
    /// Raw numeric JSON `id` when it differs from catalog `id` (often UPC in favorites).
    pub list_id: Option<u64>,
    pub genre: Option<GenreRef>,
    pub label: Option<LabelRef>,
}

impl AlbumSummary {
    pub fn matches_qobuz_catalog_id(&self, catalog_id: u64) -> bool {
        self.qobuz_id == Some(catalog_id)
            || self.id == catalog_id
            || self.list_id == Some(catalog_id)
    }

    /// Prefer non-numeric `album/get` ids when `api_album_id()` would be the catalog id string.
    pub fn pick_album_api_id(&self, catalog_id: u64) -> Option<String> {
        let api = self.api_album_id();
        if api.parse::<u64>().ok() != Some(catalog_id) {
            return Some(api);
        }
        self.album_ref
            .clone()
            .or_else(|| self.slug.clone())
            .filter(|s| !s.trim().is_empty())
    }

    /// Value for `album/get` `album_id` — short ref, then long slug, then numeric catalog id.
    pub fn api_album_id(&self) -> String {
        if let Some(r) = &self.album_ref {
            let t = r.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
        if let Some(slug) = &self.slug {
            let t = slug.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
        self.id.to_string()
    }
}

#[derive(Debug, Deserialize)]
struct AlbumSummaryRaw {
    #[serde(rename = "qobuz_id", default)]
    qobuz_id: Option<serde_json::Value>,
    id: serde_json::Value,
    #[serde(default)]
    upc: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_null_string")]
    slug: String,
    #[serde(default, deserialize_with = "deserialize_null_string")]
    title: String,
    artist: Option<ArtistRef>,
    artists: Option<Vec<ArtistRef>>,
    image: Option<Image>,
    #[serde(rename = "release_date_original")]
    release_date_original: Option<String>,
    hires: Option<bool>,
    #[serde(default)]
    genre: Option<GenreRef>,
    #[serde(default)]
    label: Option<LabelRef>,
}

impl<'de> Deserialize<'de> for AlbumSummary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = AlbumSummaryRaw::deserialize(deserializer)?;
        let album_ref = parse_album_ref_value(&raw.id);
        let explicit_qobuz = raw
            .qobuz_id
            .as_ref()
            .and_then(|v| parse_id_value(v).ok());
        let legacy_id = parse_id_value(&raw.id).ok();
        let upc = raw.upc.as_ref().and_then(|v| parse_id_value(v).ok());

        let id = explicit_qobuz
            .or_else(|| {
                legacy_id.filter(|&n| {
                    upc != Some(n) && n < MAX_FALLBACK_NUMERIC_ID
                })
            })
            .or_else(|| {
                legacy_id.filter(|_| album_ref.is_some() || !raw.slug.trim().is_empty())
            })
            .ok_or_else(|| {
                de::Error::custom(format!(
                    "album missing catalog id (id {:?}, qobuz_id {:?}, album_ref {:?}, slug {:?})",
                    raw.id, raw.qobuz_id, album_ref, raw.slug
                ))
            })?;

        let slug = (!raw.slug.trim().is_empty()).then(|| raw.slug.trim().to_string());

        let list_id = match (explicit_qobuz, legacy_id) {
            (Some(catalog), Some(raw_id)) if catalog != raw_id => Some(raw_id),
            _ => None,
        };

        Ok(Self {
            id,
            qobuz_id: explicit_qobuz,
            title: raw.title,
            artist: raw.artist,
            artists: raw.artists,
            image: raw.image,
            release_date_original: raw.release_date_original,
            hires: raw.hires,
            album_ref,
            slug,
            list_id,
            genre: raw.genre,
            label: raw.label,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_rich_album_fixture() {
        let json = include_str!("../../tests/fixtures/album_get_rich.json");
        let album: AlbumDetail = serde_json::from_str(json).unwrap();
        assert_eq!(album.summary.genre.as_ref().unwrap().name, "Classical");
        assert_eq!(album.summary.label.as_ref().unwrap().name, "Test Label");
        let t0 = &album.tracks.as_ref().unwrap().items[0];
        assert_eq!(t0.media_number, Some(1));
        assert_eq!(t0.genre.as_ref().unwrap().name, "Orchestral");
        assert_eq!(t0.isrc.as_deref(), Some("XX-XXX-19-00001"));
        assert_eq!(t0.composer.as_ref().unwrap().name, "Composer Name");
        assert_eq!(album.tracks.as_ref().unwrap().items[1].media_number, Some(2));
    }

    #[test]
    fn prefers_qobuz_id_over_legacy_id() {
        let json = r#"{
            "id": 225770297,
            "qobuz_id": 12345,
            "title": "Test"
        }"#;
        let a: AlbumSummary = serde_json::from_str(json).unwrap();
        assert_eq!(a.id, 12345);
        assert_eq!(a.api_album_id(), "12345");
    }

    #[test]
    fn string_id_with_qobuz_id_uses_short_ref_for_api() {
        let json = r#"{
            "id": "zg7pv28g4mldg",
            "qobuz_id": 393908828,
            "slug": "lutosawski-concertos-for-cello-for-orchestra-bloch-schelomo-luxembourg-philharmonic-gustavo-gimeno-jean-guihen-queyras",
            "title": "Lutosławski"
        }"#;
        let a: AlbumSummary = serde_json::from_str(json).unwrap();
        assert_eq!(a.id, 393908828);
        assert_eq!(a.album_ref.as_deref(), Some("zg7pv28g4mldg"));
        assert_eq!(a.api_album_id(), "zg7pv28g4mldg");
    }

    #[test]
    fn large_id_with_slug_uses_slug_when_no_album_ref() {
        let json = r#"{
            "id": 225770297,
            "slug": "aarab-zaraq-lucid-dreaming-therion",
            "title": "Test"
        }"#;
        let a: AlbumSummary = serde_json::from_str(json).unwrap();
        assert_eq!(a.id, 225770297);
        assert_eq!(
            a.api_album_id(),
            "aarab-zaraq-lucid-dreaming-therion"
        );
    }

    #[test]
    fn rejects_large_id_without_qobuz_id_or_slug() {
        let json = r#"{"id": 225770297, "title": "Test"}"#;
        assert!(serde_json::from_str::<AlbumSummary>(json).is_err());
    }

    #[test]
    fn pick_album_api_id_prefers_ref_over_numeric_catalog() {
        let json = r#"{
            "id": 3149020953969,
            "qobuz_id": 393908828,
            "slug": "lutosawski-concertos",
            "title": "Test"
        }"#;
        let a: AlbumSummary = serde_json::from_str(json).unwrap();
        assert_eq!(a.id, 393908828);
        assert_eq!(a.list_id, Some(3149020953969));
        assert_eq!(
            a.pick_album_api_id(393908828).as_deref(),
            Some("lutosawski-concertos")
        );
    }
}

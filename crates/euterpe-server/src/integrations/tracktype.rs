use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::error::ApiError;
use crate::integrations::provider::TagSourceProvider;
use crate::integrations::provider::lookup_page_result;
use crate::integrations::types::{
    AlbumLookupContext, AlbumLookupResult, AlbumMetadataRelease, AlbumMetadataTrack,
    MetadataCandidate,
};

const DEFAULT_API_BASE: &str = "https://tracktype.org";

pub struct TrackTypeProvider {
    client: Client,
    api_base: String,
    api_key: Option<String>,
}

impl TrackTypeProvider {
    pub fn new(api_base: Option<&str>, api_key: Option<&str>) -> Result<Self, ApiError> {
        let api_base = api_base
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_API_BASE)
            .trim_end_matches('/')
            .to_string();
        let api_key = api_key
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let client = Client::builder()
            .user_agent("Euterpe/0.1")
            .build()
            .map_err(|e| ApiError::Message(e.to_string()))?;
        Ok(Self {
            client,
            api_base,
            api_key,
        })
    }
}

#[derive(Debug, Deserialize)]
struct TrackTypeSearchResponse {
    #[serde(default)]
    results: Vec<TrackTypeResult>,
}

#[derive(Debug, Deserialize)]
struct TrackTypeResult {
    id: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    year: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TrackTypeDetail {
    artist: Option<String>,
    album: Option<String>,
    year: Option<i32>,
    genre: Option<String>,
    #[serde(default)]
    tracks: Vec<TrackTypeTrack>,
}

#[derive(Debug, Deserialize)]
struct TrackTypeTrack {
    title: Option<String>,
    track_number: Option<u32>,
}

#[async_trait]
impl TagSourceProvider for TrackTypeProvider {
    fn source_label(&self) -> &'static str {
        "TrackType.org"
    }

    async fn lookup_album(
        &self,
        ctx: &AlbumLookupContext,
        page: u32,
    ) -> Result<AlbumLookupResult, ApiError> {
        let url = format!("{}/api/search", self.api_base);
        let query = [
            ("artist", ctx.artist_name.as_str()),
            ("album", ctx.album_title.as_str()),
        ];
        let mut req = self.client.get(&url).query(&query);
        if let Some(ref key) = self.api_key {
            req = req.header("X-Api-Key", key);
        }
        let resp = req.send().await;
        let Ok(resp) = resp else {
            return Err(ApiError::Message(
                "PROVIDER_UNAVAILABLE: TrackType.org unreachable".into(),
            ));
        };
        if resp.status() == reqwest::StatusCode::NOT_FOUND
            || resp.status() == reqwest::StatusCode::NOT_IMPLEMENTED
        {
            return Err(ApiError::Message(
                "PROVIDER_UNAVAILABLE: TrackType.org API not available".into(),
            ));
        }
        if !resp.status().is_success() {
            return Err(ApiError::Message(format!(
                "TrackType search HTTP {}",
                resp.status()
            )));
        }
        let body: TrackTypeSearchResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::Message(format!("TrackType JSON: {e}")))?;
        let candidates = body
            .results
            .into_iter()
            .enumerate()
            .map(|(i, r)| MetadataCandidate {
                id: r.id.unwrap_or_else(|| format!("tracktype-{i}")),
                title: r.album.unwrap_or_else(|| ctx.album_title.clone()),
                artist_name: r.artist.unwrap_or_else(|| ctx.artist_name.clone()),
                year: r.year.or(ctx.year),
                score: (1.0 - i as f32 * 0.06).max(0.1),
                track_count: Some(ctx.tracks.len() as u32),
                source_label: self.source_label().into(),
            })
            .collect();
        Ok(lookup_page_result(page, candidates))
    }

    async fn load_release(&self, candidate_id: &str) -> Result<AlbumMetadataRelease, ApiError> {
        let url = format!("{}/api/release/{}", self.api_base, candidate_id);
        let mut req = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.header("X-Api-Key", key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Message(format!("PROVIDER_UNAVAILABLE: TrackType {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::Message(format!(
                "TrackType release HTTP {}",
                resp.status()
            )));
        }
        let d: TrackTypeDetail = resp
            .json()
            .await
            .map_err(|e| ApiError::Message(format!("TrackType detail JSON: {e}")))?;
        let tracks: Vec<AlbumMetadataTrack> = d
            .tracks
            .into_iter()
            .enumerate()
            .map(|(i, t)| AlbumMetadataTrack {
                title: t.title.unwrap_or_else(|| format!("Track {}", i + 1)),
                track_number: t.track_number.or(Some((i + 1) as u32)),
                disc_number: Some(1),
                year: d.year.map(|y| y as u32),
                genre: d.genre.clone(),
            })
            .collect();
        Ok(AlbumMetadataRelease {
            title: d.album.unwrap_or_default(),
            artist_name: d.artist.unwrap_or_default(),
            year: d.year,
            genre: d.genre,
            tracks,
            cover_url: None,
        })
    }
}

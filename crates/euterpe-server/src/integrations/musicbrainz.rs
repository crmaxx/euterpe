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

const API_BASE: &str = "https://musicbrainz.org/ws/2";

pub struct MusicBrainzProvider {
    client: Client,
    user_agent: String,
}

impl MusicBrainzProvider {
    pub fn new(contact: &str) -> Result<Self, ApiError> {
        let contact = contact.trim();
        if contact.is_empty() {
            return Err(ApiError::bad_request("MusicBrainz contact email is required"));
        }
        let user_agent = format!("Euterpe/0.1 ( {contact} )");
        let client = Client::builder()
            .user_agent(&user_agent)
            .build()
            .map_err(|e| ApiError::Message(e.to_string()))?;
        Ok(Self { client, user_agent })
    }

    fn search_query(ctx: &AlbumLookupContext) -> String {
        let artist = ctx.artist_name.replace('"', "");
        let album = ctx.album_title.replace('"', "");
        format!(r#"artist:"{artist}" AND release:"{album}""#)
    }
}

#[derive(Debug, Deserialize)]
struct MbReleaseSearch {
    releases: Option<Vec<MbReleaseSummary>>,
}

#[derive(Debug, Deserialize)]
struct MbReleaseSummary {
    id: String,
    title: String,
    #[serde(rename = "date")]
    date: Option<String>,
    #[serde(rename = "artist-credit")]
    artist_credit: Option<Vec<MbArtistCredit>>,
    #[serde(rename = "track-count")]
    track_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MbArtistCredit {
    name: String,
}

#[derive(Debug, Deserialize)]
struct MbReleaseDetail {
    #[allow(dead_code)]
    id: String,
    title: String,
    date: Option<String>,
    #[serde(rename = "artist-credit")]
    artist_credit: Option<Vec<MbArtistCredit>>,
    media: Option<Vec<MbMedia>>,
}

#[derive(Debug, Deserialize)]
struct MbMedia {
    tracks: Option<Vec<MbTrack>>,
}

#[derive(Debug, Deserialize)]
struct MbTrack {
    title: String,
    position: Option<u32>,
    number: Option<String>,
    #[allow(dead_code)]
    length: Option<u32>,
}

#[async_trait]
impl TagSourceProvider for MusicBrainzProvider {
    fn source_label(&self) -> &'static str {
        "MusicBrainz"
    }

    async fn lookup_album(
        &self,
        ctx: &AlbumLookupContext,
        page: u32,
    ) -> Result<AlbumLookupResult, ApiError> {
        let query = Self::search_query(ctx);
        let url = format!("{API_BASE}/release");
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", &self.user_agent)
            .query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "15")])
            .send()
            .await
            .map_err(|e| ApiError::Message(format!("MusicBrainz request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::Message(format!(
                "MusicBrainz search HTTP {}",
                resp.status()
            )));
        }
        let body: MbReleaseSearch = resp
            .json()
            .await
            .map_err(|e| ApiError::Message(format!("MusicBrainz JSON: {e}")))?;
        let releases = body.releases.unwrap_or_default();
        let mut out = Vec::new();
        for (i, rel) in releases.into_iter().enumerate() {
            let artist_name = rel
                .artist_credit
                .as_ref()
                .and_then(|c| c.first())
                .map(|a| a.name.clone())
                .unwrap_or_else(|| ctx.artist_name.clone());
            let year = rel
                .date
                .as_deref()
                .and_then(|d| d.get(0..4))
                .and_then(|y| y.parse().ok());
            let score = 1.0 - (i as f32 * 0.05);
            out.push(MetadataCandidate {
                id: rel.id,
                title: rel.title,
                artist_name,
                year,
                score: score.max(0.1),
                track_count: rel.track_count,
                source_label: self.source_label().into(),
            });
        }
        Ok(lookup_page_result(page, out))
    }

    async fn load_release(&self, candidate_id: &str) -> Result<AlbumMetadataRelease, ApiError> {
        let url = format!("{API_BASE}/release/{candidate_id}");
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", &self.user_agent)
            .query(&[("inc", "recordings+artist-credits"), ("fmt", "json")])
            .send()
            .await
            .map_err(|e| ApiError::Message(format!("MusicBrainz release: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::Message(format!(
                "MusicBrainz release HTTP {}",
                resp.status()
            )));
        }
        let rel: MbReleaseDetail = resp
            .json()
            .await
            .map_err(|e| ApiError::Message(format!("MusicBrainz release JSON: {e}")))?;
        let artist_name = rel
            .artist_credit
            .as_ref()
            .and_then(|c| c.first())
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let year = rel
            .date
            .as_deref()
            .and_then(|d| d.get(0..4))
            .and_then(|y| y.parse().ok())
            .map(|y: u32| y);
        let mut tracks = Vec::new();
        let mut track_num = 0u32;
        for media in rel.media.unwrap_or_default() {
            for t in media.tracks.unwrap_or_default() {
                track_num += 1;
                let tn = t
                    .position
                    .or_else(|| t.number.and_then(|n| n.parse().ok()));
                tracks.push(AlbumMetadataTrack {
                    title: t.title,
                    track_number: Some(tn.unwrap_or(track_num)),
                    disc_number: Some(1),
                    year,
                    genre: None,
                });
            }
        }
        Ok(AlbumMetadataRelease {
            title: rel.title,
            artist_name,
            year: year.map(|y| y as i32),
            genre: None,
            tracks,
            cover_url: None,
        })
    }
}

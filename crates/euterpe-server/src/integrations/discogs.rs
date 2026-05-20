use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::error::ApiError;
use crate::integrations::provider::TagSourceProvider;
use crate::integrations::types::{
    AlbumLookupContext, AlbumLookupResult, AlbumMetadataRelease, AlbumMetadataTrack,
    MetadataCandidate,
};

const DISCOGS_PER_PAGE: u32 = 15;

const API_BASE: &str = "https://api.discogs.com";

pub struct DiscogsProvider {
    client: Client,
    token: String,
}

impl DiscogsProvider {
    pub fn new(token: &str) -> Result<Self, ApiError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(ApiError::bad_request("Discogs token is required"));
        }
        let client = Client::builder()
            .user_agent("Euterpe/0.1 +https://github.com/crmaxx/euterpe")
            .build()
            .map_err(|e| ApiError::Message(e.to_string()))?;
        Ok(Self {
            client,
            token: token.into(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct DiscogsSearchResponse {
    pagination: Option<DiscogsPagination>,
    results: Option<Vec<DiscogsSearchResult>>,
}

#[derive(Debug, Deserialize)]
struct DiscogsPagination {
    page: u32,
    pages: u32,
}

#[derive(Debug, Deserialize)]
struct DiscogsSearchResult {
    id: u64,
    title: String,
    #[serde(rename = "type")]
    type_: Option<String>,
    year: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscogsRelease {
    #[allow(dead_code)]
    id: u64,
    title: String,
    year: Option<u32>,
    styles: Option<Vec<String>>,
    artists: Option<Vec<DiscogsArtist>>,
    tracklist: Option<Vec<DiscogsTrack>>,
    images: Option<Vec<DiscogsImage>>,
}

#[derive(Debug, Deserialize)]
struct DiscogsArtist {
    name: String,
}

#[derive(Debug, Deserialize)]
struct DiscogsTrack {
    title: String,
    position: Option<String>,
    #[serde(rename = "type")]
    type_: Option<String>,
    #[serde(default)]
    sub_tracks: Option<Vec<DiscogsTrack>>,
}

#[derive(Debug, Deserialize)]
struct DiscogsImage {
    #[serde(rename = "type")]
    type_: String,
    uri: String,
}

#[async_trait]
impl TagSourceProvider for DiscogsProvider {
    fn source_label(&self) -> &'static str {
        "Discogs"
    }

    async fn lookup_album(
        &self,
        ctx: &AlbumLookupContext,
        page: u32,
    ) -> Result<AlbumLookupResult, ApiError> {
        let page = page.max(1);
        let q = format!("{} {}", ctx.artist_name, ctx.album_title);
        let page_s = page.to_string();
        let per_page_s = DISCOGS_PER_PAGE.to_string();
        let mut req = self
            .client
            .get(format!("{API_BASE}/database/search"))
            .query(&[
                ("q", q.as_str()),
                ("type", "release"),
                ("per_page", per_page_s.as_str()),
                ("page", page_s.as_str()),
            ]);
        if let Some(year) = ctx.year {
            let year_s = year.to_string();
            req = req.query(&[("year", year_s.as_str())]);
        }
        let resp = req
            .header("Authorization", format!("Discogs token={}", self.token))
            .send()
            .await
            .map_err(|e| ApiError::Message(format!("Discogs search: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::Message(format!(
                "Discogs search HTTP {}",
                resp.status()
            )));
        }
        let body: DiscogsSearchResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::Message(format!("Discogs JSON: {e}")))?;
        let page = body.pagination.as_ref().map(|p| p.page).unwrap_or(page);
        let has_more = body.pagination.as_ref().is_some_and(|p| p.page < p.pages);
        let offset = (page.saturating_sub(1)) * DISCOGS_PER_PAGE;
        let mut out = Vec::new();
        for (i, r) in body.results.unwrap_or_default().into_iter().enumerate() {
            if r.type_.as_deref() != Some("release") {
                continue;
            }
            let (artist_name, title) = split_discogs_title(&r.title);
            let year = r.year.and_then(|y| y.parse().ok());
            let rank = offset + i as u32;
            out.push(MetadataCandidate {
                id: r.id.to_string(),
                title,
                artist_name: if artist_name.is_empty() {
                    ctx.artist_name.clone()
                } else {
                    artist_name
                },
                year,
                score: (1.0 - rank as f32 * 0.05).max(0.1),
                track_count: None,
                source_label: self.source_label().into(),
            });
        }
        Ok(AlbumLookupResult {
            candidates: out,
            page,
            has_more,
        })
    }

    async fn load_release(&self, candidate_id: &str) -> Result<AlbumMetadataRelease, ApiError> {
        let resp = self
            .client
            .get(format!("{API_BASE}/releases/{candidate_id}"))
            .header("Authorization", format!("Discogs token={}", self.token))
            .send()
            .await
            .map_err(|e| ApiError::Message(format!("Discogs release: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::Message(format!(
                "Discogs release HTTP {}",
                resp.status()
            )));
        }
        let rel: DiscogsRelease = resp
            .json()
            .await
            .map_err(|e| ApiError::Message(format!("Discogs release JSON: {e}")))?;
        let artist_name = rel
            .artists
            .as_ref()
            .and_then(|a| a.first())
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let genre = styles_as_genre(&rel.styles);
        let cover_url = rel.images.as_ref().and_then(|imgs| {
            imgs.iter()
                .find(|i| i.type_ == "primary" || i.type_ == "secondary")
                .map(|i| i.uri.clone())
        });
        let flat = flatten_discogs_tracklist(rel.tracklist.unwrap_or_default());
        let mut track_num = 0u32;
        let tracks: Vec<AlbumMetadataTrack> = flat
            .into_iter()
            .map(|(title, _position)| {
                track_num += 1;
                AlbumMetadataTrack {
                    title,
                    track_number: Some(track_num),
                    disc_number: Some(1),
                    year: rel.year,
                    genre: genre.clone(),
                }
            })
            .collect();
        Ok(AlbumMetadataRelease {
            title: rel.title,
            artist_name,
            year: rel.year.map(|y| y as i32),
            genre,
            tracks,
            cover_url,
        })
    }
}

/// Expand index tracks and skip headings so movements match per-file rips.
fn flatten_discogs_tracklist(items: Vec<DiscogsTrack>) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for item in items {
        push_discogs_playable_tracks(&mut out, item);
    }
    out
}

fn push_discogs_playable_tracks(out: &mut Vec<(String, Option<String>)>, t: DiscogsTrack) {
    let ty = t.type_.as_deref().unwrap_or("track").to_ascii_lowercase();
    if ty == "heading" {
        return;
    }
    if ty == "index" {
        if let Some(subs) = t.sub_tracks {
            for st in subs {
                push_discogs_playable_tracks(out, st);
            }
        }
        return;
    }
    if t.sub_tracks.as_ref().is_some_and(|s| !s.is_empty()) {
        for st in t.sub_tracks.unwrap_or_default() {
            push_discogs_playable_tracks(out, st);
        }
        return;
    }
    if t.title.is_empty() || (t.title.starts_with('[') && t.title.contains("Album")) {
        return;
    }
    out.push((t.title, t.position));
}

fn styles_as_genre(styles: &Option<Vec<String>>) -> Option<String> {
    let joined = styles
        .as_ref()?
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

fn split_discogs_title(title: &str) -> (String, String) {
    if let Some((a, t)) = title.split_once(" - ") {
        return (a.trim().to_string(), t.trim().to_string());
    }
    (String::new(), title.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_has_more_when_more_pages() {
        let body: DiscogsSearchResponse = serde_json::from_str(
            r#"{
                "pagination": { "page": 1, "pages": 3 },
                "results": []
            }"#,
        )
        .unwrap();
        let has_more = body.pagination.as_ref().is_some_and(|p| p.page < p.pages);
        assert!(has_more);
    }

    #[test]
    fn flatten_expands_index_sub_tracks() {
        let json = r#"[
            {"type":"track","title":"Concerto For Orchestra","position":"1"},
            {"type":"track","title":"Schelomo","position":"2"},
            {
                "type":"index",
                "title":"Concerto For Cello",
                "sub_tracks":[
                    {"type":"track","title":"I. Lento moderato","position":"3-1"},
                    {"type":"track","title":"II. Allegro moderato","position":"3-2"}
                ]
            }
        ]"#;
        let items: Vec<DiscogsTrack> = serde_json::from_str(json).unwrap();
        let flat = flatten_discogs_tracklist(items);
        assert_eq!(flat.len(), 4);
        assert_eq!(flat[2].0, "I. Lento moderato");
        assert_eq!(flat[3].0, "II. Allegro moderato");
    }

    #[test]
    fn release_joins_all_styles_as_genre() {
        let rel: DiscogsRelease = serde_json::from_str(
            r#"{
                "id": 1,
                "title": "Album",
                "year": 2020,
                "styles": ["Black Metal", "Death Metal"],
                "artists": [{"name": "Band"}],
                "tracklist": [{"title": "Song", "position": "1"}]
            }"#,
        )
        .unwrap();
        assert_eq!(
            styles_as_genre(&rel.styles).as_deref(),
            Some("Black Metal, Death Metal")
        );
    }
}

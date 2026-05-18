use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::error::ApiError;
use crate::integrations::provider::{lookup_page_result, TagSourceProvider};
use crate::integrations::types::{
    AlbumLookupContext, AlbumLookupResult, AlbumMetadataRelease, AlbumMetadataTrack,
    MetadataCandidate,
};

const DEFAULT_SERVER: &str = "http://gnudb.gnudb.org:80";

pub struct GnudbProvider {
    client: Client,
    server_base: String,
}

impl GnudbProvider {
    pub fn new(server_base: Option<&str>) -> Result<Self, ApiError> {
        let base = server_base
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_SERVER)
            .trim_end_matches('/')
            .to_string();
        let client = Client::builder()
            .user_agent("Euterpe/0.1")
            .build()
            .map_err(|e| ApiError::Message(e.to_string()))?;
        Ok(Self {
            client,
            server_base: base,
        })
    }

    fn toc_query(ctx: &AlbumLookupContext) -> String {
        let mut parts: Vec<String> = Vec::new();
        for t in &ctx.tracks {
            if let Some(sec) = t.duration_sec.filter(|&s| s > 0) {
                parts.push(format!("{sec}"));
            }
        }
        if parts.is_empty() {
            return format!(
                "cat=album&artist={}&title={}",
                urlencoding_sim(&ctx.artist_name),
                urlencoding_sim(&ctx.album_title)
            );
        }
        format!("cat=toc&t={}", parts.join("+"))
    }
}

fn urlencoding_sim(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct FreedbSearchResponse {
    #[serde(default)]
    data: Vec<FreedbEntry>,
}

#[derive(Debug, Deserialize)]
struct FreedbEntry {
    #[allow(dead_code)]
    category: Option<String>,
    disc_id: Option<String>,
    title: Option<String>,
}

#[async_trait]
impl TagSourceProvider for GnudbProvider {
    fn source_label(&self) -> &'static str {
        "GnuDB"
    }

    async fn lookup_album(
        &self,
        ctx: &AlbumLookupContext,
        page: u32,
    ) -> Result<AlbumLookupResult, ApiError> {
        if page > 1 {
            return Ok(lookup_page_result(page, vec![]));
        }
        let url = format!("{}/~cymac/freedb_search_json.php", self.server_base);
        let query = Self::toc_query(ctx);
        let resp = self
            .client
            .get(&url)
            .query(&[("s", query.as_str())])
            .send()
            .await;
        let resp = match resp {
            Ok(r) => r,
            Err(_) => {
                return text_search_fallback(&self.client, &self.server_base, ctx, page).await;
            }
        };
        if !resp.status().is_success() {
            return text_search_fallback(&self.client, &self.server_base, ctx, page).await;
        }
        let body: FreedbSearchResponse = match resp.json().await {
            Ok(b) => b,
            Err(_) => return text_search_fallback(&self.client, &self.server_base, ctx, page).await,
        };
        let mut out = Vec::new();
        for (i, entry) in body.data.into_iter().enumerate() {
            let title = entry.title.unwrap_or_else(|| ctx.album_title.clone());
            let (artist_name, album_title) = parse_freedb_title(&title, ctx);
            let id = entry.disc_id.unwrap_or_else(|| format!("gnudb-{i}"));
            out.push(MetadataCandidate {
                id,
                title: album_title,
                artist_name,
                year: ctx.year,
                score: (1.0 - i as f32 * 0.08).max(0.1),
                track_count: Some(ctx.tracks.len() as u32),
                source_label: self.source_label().into(),
            });
        }
        if out.is_empty() {
            return text_search_fallback(&self.client, &self.server_base, ctx, page).await;
        }
        Ok(lookup_page_result(page, out))
    }

    async fn load_release(&self, candidate_id: &str) -> Result<AlbumMetadataRelease, ApiError> {
        let url = format!("{}/~cymac/freedb_read_json.php", self.server_base);
        let resp = self
            .client
            .get(&url)
            .query(&[("disc_id", candidate_id)])
            .send()
            .await
            .map_err(|e| ApiError::Message(format!("GnuDB read: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::Message(format!(
                "PROVIDER_UNAVAILABLE: GnuDB read HTTP {}",
                resp.status()
            )));
        }
        #[derive(Debug, Deserialize)]
        struct ReadResponse {
            title: Option<String>,
            tracks: Option<Vec<GnudbTrack>>,
        }
        #[derive(Debug, Deserialize)]
        struct GnudbTrack {
            title: Option<String>,
            track: Option<u32>,
        }
        let body: ReadResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::Message(format!("GnuDB JSON: {e}")))?;
        let title = body.title.unwrap_or_default();
        let (artist_name, album_title) = parse_freedb_title(
            &title,
            &AlbumLookupContext {
                artist_name: String::new(),
                album_title: String::new(),
                year: None,
                tracks: vec![],
            },
        );
        let tracks: Vec<AlbumMetadataTrack> = body
            .tracks
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, t)| AlbumMetadataTrack {
                title: t.title.unwrap_or_else(|| format!("Track {}", i + 1)),
                track_number: t.track.or(Some((i + 1) as u32)),
                disc_number: Some(1),
                year: None,
                genre: None,
            })
            .collect();
        Ok(AlbumMetadataRelease {
            title: album_title,
            artist_name,
            year: None,
            genre: None,
            tracks,
            cover_url: None,
        })
    }
}

async fn text_search_fallback(
    client: &Client,
    server_base: &str,
    ctx: &AlbumLookupContext,
    page: u32,
) -> Result<AlbumLookupResult, ApiError> {
    let q = format!(
        "cat=album&artist={}&title={}",
        urlencoding_sim(&ctx.artist_name),
        urlencoding_sim(&ctx.album_title)
    );
    let url = format!("{server_base}/~cymac/freedb_search_json.php");
    let resp = client.get(&url).query(&[("s", q.as_str())]).send().await;
    let Ok(resp) = resp else {
        return Ok(lookup_page_result(page, vec![]));
    };
    if !resp.status().is_success() {
        return Ok(lookup_page_result(page, vec![]));
    }
    let body: FreedbSearchResponse = match resp.json().await {
        Ok(b) => b,
        Err(_) => return Ok(lookup_page_result(page, vec![])),
    };
    let candidates = body
        .data
        .into_iter()
        .enumerate()
        .map(|(i, entry)| {
            let title = entry.title.unwrap_or_else(|| ctx.album_title.clone());
            let (artist_name, album_title) = parse_freedb_title(&title, ctx);
            MetadataCandidate {
                id: entry.disc_id.unwrap_or_else(|| format!("gnudb-{i}")),
                title: album_title,
                artist_name,
                year: ctx.year,
                score: (0.9 - i as f32 * 0.08).max(0.1),
                track_count: Some(ctx.tracks.len() as u32),
                source_label: "GnuDB".into(),
            }
        })
        .collect();
    Ok(lookup_page_result(page, candidates))
}

fn parse_freedb_title(title: &str, ctx: &AlbumLookupContext) -> (String, String) {
    if let Some((a, t)) = title.split_once('/') {
        return (a.trim().to_string(), t.trim().to_string());
    }
    if let Some((a, t)) = title.split_once(" - ") {
        return (a.trim().to_string(), t.trim().to_string());
    }
    (ctx.artist_name.clone(), title.trim().to_string())
}

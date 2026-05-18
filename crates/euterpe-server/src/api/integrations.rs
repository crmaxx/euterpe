use serde::{Deserialize, Serialize};

use crate::integrations::catalog::IntegrationCatalogEntry;

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationListItem {
    pub id: i64,
    pub integration_type: String,
    pub provider: String,
    pub display_name: String,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub has_secrets: bool,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationsListResponse {
    pub items: Vec<IntegrationListItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationsCatalogResponse {
    pub items: Vec<IntegrationCatalogEntry>,
}

#[derive(Debug, Deserialize)]
pub struct IntegrationCreateRequest {
    pub provider: String,
    #[serde(rename = "type")]
    pub integration_type: String,
    pub display_name: Option<String>,
    pub enabled: Option<bool>,
    pub config: serde_json::Value,
    pub secrets: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct IntegrationPatchRequest {
    pub display_name: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
    pub secrets: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationResponse {
    pub item: IntegrationListItem,
}

#[derive(Debug, Deserialize)]
pub struct AlbumMetadataLookupRequest {
    pub integration_id: i64,
    #[serde(default = "default_lookup_page")]
    pub page: u32,
}

fn default_lookup_page() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize)]
pub struct AlbumMetadataLookupResponse {
    pub candidates: Vec<crate::integrations::MetadataCandidate>,
    pub page: u32,
    pub has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct AlbumMetadataApplyRequest {
    pub integration_id: i64,
    pub candidate_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlbumMetadataApplyResponse {
    pub tracks_updated: u32,
    pub cover_applied: bool,
    pub warnings: Vec<String>,
}

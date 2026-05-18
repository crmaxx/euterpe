use async_trait::async_trait;

use crate::error::ApiError;
use crate::integrations::types::{
    AlbumLookupContext, AlbumLookupResult, AlbumMetadataRelease, MetadataCandidate,
};

pub(crate) fn lookup_page_result(
    page: u32,
    candidates: Vec<MetadataCandidate>,
) -> AlbumLookupResult {
    AlbumLookupResult {
        candidates: if page <= 1 { candidates } else { Vec::new() },
        page: page.max(1),
        has_more: false,
    }
}

#[async_trait]
pub trait TagSourceProvider: Send + Sync {
    fn source_label(&self) -> &'static str;

    async fn lookup_album(
        &self,
        ctx: &AlbumLookupContext,
        page: u32,
    ) -> Result<AlbumLookupResult, ApiError>;

    async fn load_release(&self, candidate_id: &str) -> Result<AlbumMetadataRelease, ApiError>;
}

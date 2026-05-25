use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::api::{
    AlbumMetadataApplyRequest, AlbumMetadataApplyResponse, AlbumMetadataLookupRequest,
    AlbumMetadataLookupResponse, IntegrationCreateRequest, IntegrationPatchRequest,
    IntegrationResponse, IntegrationsCatalogResponse, IntegrationsListResponse,
};
use crate::error::ApiError;
use crate::integrations::apply::{self, build_lookup_context};
use crate::integrations::catalog::{IntegrationType, catalog_entries};
use crate::integrations::registry::build_tag_source;
use crate::services::integrations as svc;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct IntegrationsListQuery {
    #[serde(rename = "type")]
    pub integration_type: Option<String>,
}

pub async fn list_integrations(
    State(state): State<AppState>,
    Query(q): Query<IntegrationsListQuery>,
) -> Result<Json<IntegrationsListResponse>, ApiError> {
    let items = svc::list_integrations(&state.db, q.integration_type.as_deref()).await?;
    Ok(Json(IntegrationsListResponse { items }))
}

pub async fn integrations_catalog(
    Query(q): Query<IntegrationsListQuery>,
) -> Result<Json<IntegrationsCatalogResponse>, ApiError> {
    let t = match q.integration_type.as_deref() {
        None => None,
        Some(s) => {
            Some(IntegrationType::parse(s).ok_or_else(|| ApiError::bad_request("invalid type"))?)
        }
    };
    Ok(Json(IntegrationsCatalogResponse {
        items: catalog_entries(t),
    }))
}

pub async fn create_integration(
    State(state): State<AppState>,
    Json(body): Json<IntegrationCreateRequest>,
) -> Result<(StatusCode, Json<IntegrationResponse>), ApiError> {
    let resp = svc::create_integration(&state.config, &state.db, body).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

pub async fn patch_integration(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<IntegrationPatchRequest>,
) -> Result<Json<IntegrationResponse>, ApiError> {
    Ok(Json(
        svc::patch_integration(&state.config, &state.db, id, body).await?,
    ))
}

pub async fn delete_integration(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    svc::delete_integration(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn album_metadata_lookup(
    State(state): State<AppState>,
    Path(album_id): Path<i64>,
    Json(body): Json<AlbumMetadataLookupRequest>,
) -> Result<Json<AlbumMetadataLookupResponse>, ApiError> {
    let row = crate::db::integrations::get_by_id(&state.db, body.integration_id)
        .await?
        .ok_or_else(|| ApiError::Message("integration not found".into()))?;
    if row.enabled == 0 {
        return Err(ApiError::bad_request("integration is disabled"));
    }
    let provider = build_tag_source(&row, state.config.master_key.as_ref())?;
    let ctx = build_lookup_context(&state.db, album_id).await?;
    let page = body.page.max(1);
    let result = provider.lookup_album(&ctx, page).await?;
    Ok(Json(AlbumMetadataLookupResponse {
        candidates: result.candidates,
        page: result.page,
        has_more: result.has_more,
    }))
}

pub async fn album_metadata_apply(
    State(state): State<AppState>,
    Path(album_id): Path<i64>,
    Json(body): Json<AlbumMetadataApplyRequest>,
) -> Result<Json<AlbumMetadataApplyResponse>, ApiError> {
    let row = crate::db::integrations::get_by_id(&state.db, body.integration_id)
        .await?
        .ok_or_else(|| ApiError::Message("integration not found".into()))?;
    if row.enabled == 0 {
        return Err(ApiError::bad_request("integration is disabled"));
    }
    let storage = state.library_storage().await?;
    let provider = build_tag_source(&row, state.config.master_key.as_ref())?;
    let release = provider.load_release(&body.candidate_id).await?;
    let result = apply::apply_release_to_album(
        &apply::ApplyStorageDeps { storage },
        &state.db,
        &state.http,
        album_id,
        &release,
    )
    .await?;
    Ok(Json(AlbumMetadataApplyResponse {
        tracks_updated: result.tracks_updated,
        cover_applied: result.cover_applied,
        warnings: result.warnings,
    }))
}

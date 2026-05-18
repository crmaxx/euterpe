use serde_json::Value;
use sqlx::SqlitePool;

use crate::api::{
    IntegrationCreateRequest, IntegrationListItem, IntegrationPatchRequest, IntegrationResponse,
};
use crate::config::AppConfig;
use crate::db::integrations::{self, IntegrationInsert, IntegrationRow, IntegrationUpdate};
use crate::error::ApiError;
use crate::integrations::catalog::{
    catalog_entries, default_display_name, IntegrationProvider, IntegrationType,
};
use crate::integrations::registry::{encrypt_secrets, validate_config};

pub fn row_to_item(row: &IntegrationRow) -> IntegrationListItem {
    let config: Value = serde_json::from_str(&row.config_json).unwrap_or(Value::Object(Default::default()));
    IntegrationListItem {
        id: row.id,
        integration_type: row.type_.clone(),
        provider: row.provider.clone(),
        display_name: row.display_name.clone(),
        enabled: row.enabled != 0,
        config,
        has_secrets: row.config_secrets_enc.is_some(),
        sort_order: row.sort_order as i32,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

pub async fn list_integrations(
    pool: &SqlitePool,
    type_filter: Option<&str>,
) -> Result<Vec<IntegrationListItem>, ApiError> {
    let t = match type_filter {
        None => None,
        Some(s) => Some(
            IntegrationType::parse(s)
                .ok_or_else(|| ApiError::bad_request("invalid type filter"))?,
        ),
    };
    let rows = integrations::list(pool, t).await?;
    Ok(rows.iter().map(row_to_item).collect())
}

pub async fn create_integration(
    config: &AppConfig,
    pool: &SqlitePool,
    body: IntegrationCreateRequest,
) -> Result<IntegrationResponse, ApiError> {
    let integration_type = IntegrationType::parse(&body.integration_type)
        .ok_or_else(|| ApiError::bad_request("invalid integration type"))?;
    let provider = IntegrationProvider::parse(&body.provider)
        .ok_or_else(|| ApiError::bad_request("unknown provider"))?;
    if !catalog_entries(Some(integration_type))
        .iter()
        .any(|e| e.provider == body.provider)
    {
        return Err(ApiError::bad_request("provider not in catalog"));
    }
    validate_config(
        provider,
        &body.config,
        body.secrets.as_ref(),
        config.master_key.as_ref(),
    )?;

    let secrets_enc = if let Some(secrets) = body.secrets {
        let master = config.master_key.as_ref().ok_or_else(|| {
            ApiError::Message("EUTERPE_MASTER_KEY is required for secrets".into())
        })?;
        Some(encrypt_secrets(master, &secrets)?)
    } else {
        None
    };

    let config_json = serde_json::to_string(&body.config)
        .map_err(|e| ApiError::Message(e.to_string()))?;
    let display_name = body
        .display_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_display_name(provider).to_string());
    let sort_order = integrations::max_sort_order(pool, integration_type).await?;

    let id = integrations::insert(
        pool,
        IntegrationInsert {
            type_: integration_type,
            provider,
            display_name: &display_name,
            enabled: body.enabled.unwrap_or(true),
            config_json: &config_json,
            config_secrets_enc: secrets_enc.as_deref(),
            sort_order,
        },
    )
    .await?;

    let row = integrations::get_by_id(pool, id)
        .await?
        .ok_or_else(|| ApiError::Message("integration not found".into()))?;
    Ok(IntegrationResponse {
        item: row_to_item(&row),
    })
}

pub async fn patch_integration(
    config: &AppConfig,
    pool: &SqlitePool,
    id: i64,
    body: IntegrationPatchRequest,
) -> Result<IntegrationResponse, ApiError> {
    let existing = integrations::get_by_id(pool, id)
        .await?
        .ok_or_else(|| ApiError::Message("integration not found".into()))?;
    let provider = IntegrationProvider::parse(&existing.provider)
        .ok_or_else(|| ApiError::bad_request("unknown provider"))?;

    let mut config_val: Value =
        serde_json::from_str(&existing.config_json).unwrap_or(Value::Object(Default::default()));
    if let Some(c) = body.config {
        config_val = c;
    }

    let secrets_patch = body.secrets;
    validate_config(
        provider,
        &config_val,
        secrets_patch.as_ref(),
        config.master_key.as_ref(),
    )?;

    let config_json = serde_json::to_string(&config_val)
        .map_err(|e| ApiError::Message(e.to_string()))?;

    let secrets_enc = if let Some(secrets) = secrets_patch {
        let master = config.master_key.as_ref().ok_or_else(|| {
            ApiError::Message("EUTERPE_MASTER_KEY is required for secrets".into())
        })?;
        Some(Some(encrypt_secrets(master, &secrets)?))
    } else {
        None
    };

    integrations::update(
        pool,
        id,
        IntegrationUpdate {
            display_name: body.display_name.as_deref(),
            enabled: body.enabled,
            config_json: Some(&config_json),
            config_secrets_enc: secrets_enc,
            sort_order: None,
        },
    )
    .await?;

    let row = integrations::get_by_id(pool, id)
        .await?
        .ok_or_else(|| ApiError::Message("integration not found".into()))?;
    Ok(IntegrationResponse {
        item: row_to_item(&row),
    })
}

pub async fn delete_integration(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    if !integrations::delete(pool, id).await? {
        return Err(ApiError::Message("integration not found".into()));
    }
    Ok(())
}

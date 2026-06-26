use crate::connection::DataHandle;
use crate::error::{DataError, Result};
use welds::WeldsModel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRow {
    pub id: i64,
    pub type_: String,
    pub provider: String,
    pub display_name: String,
    pub enabled: i64,
    pub config_json: String,
    pub config_secrets_enc: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub struct IntegrationInsert<'a> {
    pub type_: &'a str,
    pub provider: &'a str,
    pub display_name: &'a str,
    pub enabled: bool,
    pub config_json: &'a str,
    pub config_secrets_enc: Option<&'a str>,
    pub sort_order: i32,
}

pub struct IntegrationUpdate<'a> {
    pub display_name: Option<&'a str>,
    pub enabled: Option<bool>,
    pub config_json: Option<&'a str>,
    pub config_secrets_enc: Option<Option<String>>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "integrations")]
struct Integration {
    #[welds(primary_key)]
    id: i64,
    #[welds(rename = "type")]
    type_: String,
    provider: String,
    display_name: String,
    enabled: i64,
    config_json: String,
    config_secrets_enc: Option<String>,
    sort_order: i64,
    created_at: String,
    updated_at: String,
}

pub async fn list(handle: &DataHandle, type_filter: Option<&str>) -> Result<Vec<IntegrationRow>> {
    let mut rows = Integration::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|integration| type_filter.is_none_or(|type_| integration.type_ == type_))
        .map(row_from_model)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| (row.sort_order, row.id));
    Ok(rows)
}

pub async fn get_by_id(handle: &DataHandle, id: i64) -> Result<Option<IntegrationRow>> {
    Ok(Integration::find_by_id(handle.client(), id)
        .await?
        .map(row_from_model))
}

pub async fn insert(handle: &DataHandle, row: IntegrationInsert<'_>) -> Result<i64> {
    let now = sqlite_timestamp();
    let mut integration = Integration::new();
    integration.type_ = row.type_.to_string();
    integration.provider = row.provider.to_string();
    integration.display_name = row.display_name.to_string();
    integration.enabled = if row.enabled { 1 } else { 0 };
    integration.config_json = row.config_json.to_string();
    integration.config_secrets_enc = row.config_secrets_enc.map(ToString::to_string);
    integration.sort_order = i64::from(row.sort_order);
    integration.created_at = now.clone();
    integration.updated_at = now;
    integration.save(handle.client()).await?;
    Ok(integration.id)
}

pub async fn update(handle: &DataHandle, id: i64, patch: IntegrationUpdate<'_>) -> Result<()> {
    let Some(mut integration) = Integration::find_by_id(handle.client(), id).await? else {
        return Err(DataError::InvalidOperation(
            "integration not found".to_string(),
        ));
    };

    if let Some(display_name) = patch.display_name {
        integration.display_name = display_name.to_string();
    }
    if let Some(enabled) = patch.enabled {
        integration.enabled = if enabled { 1 } else { 0 };
    }
    if let Some(config_json) = patch.config_json {
        integration.config_json = config_json.to_string();
    }
    if let Some(config_secrets_enc) = patch.config_secrets_enc {
        integration.config_secrets_enc = config_secrets_enc;
    }
    if let Some(sort_order) = patch.sort_order {
        integration.sort_order = i64::from(sort_order);
    }
    integration.updated_at = sqlite_timestamp();
    integration.save(handle.client()).await?;
    Ok(())
}

pub async fn delete(handle: &DataHandle, id: i64) -> Result<bool> {
    let Some(mut integration) = Integration::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    integration.delete(handle.client()).await?;
    Ok(true)
}

pub async fn max_sort_order(handle: &DataHandle, type_: &str) -> Result<i32> {
    Ok(Integration::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|integration| integration.type_ == type_)
        .map(|integration| integration.sort_order)
        .max()
        .map(|max| max as i32 + 1)
        .unwrap_or(0))
}

fn row_from_model(integration: welds::state::DbState<Integration>) -> IntegrationRow {
    IntegrationRow {
        id: integration.id,
        type_: integration.type_.clone(),
        provider: integration.provider.clone(),
        display_name: integration.display_name.clone(),
        enabled: integration.enabled,
        config_json: integration.config_json.clone(),
        config_secrets_enc: integration.config_secrets_enc.clone(),
        sort_order: integration.sort_order,
        created_at: integration.created_at.clone(),
        updated_at: integration.updated_at.clone(),
    }
}

fn sqlite_timestamp() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

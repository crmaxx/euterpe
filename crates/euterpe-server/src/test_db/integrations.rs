use euterpe_data::DataHandle;
use euterpe_data::repositories::integrations as data;
use sqlx::SqlitePool;

use crate::error::ApiError;
use crate::integrations::catalog::{IntegrationProvider, IntegrationType};

#[derive(Debug, Clone)]
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
    pub type_: IntegrationType,
    pub provider: IntegrationProvider,
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

pub async fn list(
    pool: &SqlitePool,
    type_filter: Option<IntegrationType>,
) -> Result<Vec<IntegrationRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(
        data::list(&handle, type_filter.map(IntegrationType::as_str))
            .await?
            .into_iter()
            .map(row_from_data)
            .collect(),
    )
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<IntegrationRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::get_by_id(&handle, id).await?.map(row_from_data))
}

pub async fn insert(pool: &SqlitePool, row: IntegrationInsert<'_>) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::insert(
        &handle,
        data::IntegrationInsert {
            type_: row.type_.as_str(),
            provider: row.provider.as_str(),
            display_name: row.display_name,
            enabled: row.enabled,
            config_json: row.config_json,
            config_secrets_enc: row.config_secrets_enc,
            sort_order: row.sort_order,
        },
    )
    .await?)
}

pub async fn update(
    pool: &SqlitePool,
    id: i64,
    patch: IntegrationUpdate<'_>,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::update(
        &handle,
        id,
        data::IntegrationUpdate {
            display_name: patch.display_name,
            enabled: patch.enabled,
            config_json: patch.config_json,
            config_secrets_enc: patch.config_secrets_enc,
            sort_order: patch.sort_order,
        },
    )
    .await?)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::delete(&handle, id).await?)
}

pub async fn max_sort_order(pool: &SqlitePool, type_: IntegrationType) -> Result<i32, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::max_sort_order(&handle, type_.as_str()).await?)
}

fn row_from_data(row: data::IntegrationRow) -> IntegrationRow {
    IntegrationRow {
        id: row.id,
        type_: row.type_,
        provider: row.provider,
        display_name: row.display_name,
        enabled: row.enabled,
        config_json: row.config_json,
        config_secrets_enc: row.config_secrets_enc,
        sort_order: row.sort_order,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

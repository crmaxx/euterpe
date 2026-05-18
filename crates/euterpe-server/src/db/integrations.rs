use sqlx::SqlitePool;

use crate::error::ApiError;
use crate::integrations::catalog::{IntegrationProvider, IntegrationType};

#[derive(Debug, Clone, sqlx::FromRow)]
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
    let rows = if let Some(t) = type_filter {
        sqlx::query_as::<_, IntegrationRow>(
            r#"
            SELECT id, type AS type_, provider, display_name, enabled, config_json,
                   config_secrets_enc, sort_order, created_at, updated_at
            FROM integrations
            WHERE type = ?
            ORDER BY sort_order ASC, id ASC
            "#,
        )
        .bind(t.as_str())
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, IntegrationRow>(
            r#"
            SELECT id, type AS type_, provider, display_name, enabled, config_json,
                   config_secrets_enc, sort_order, created_at, updated_at
            FROM integrations
            ORDER BY sort_order ASC, id ASC
            "#,
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<IntegrationRow>, ApiError> {
    let row = sqlx::query_as::<_, IntegrationRow>(
        r#"
        SELECT id, type AS type_, provider, display_name, enabled, config_json,
               config_secrets_enc, sort_order, created_at, updated_at
        FROM integrations
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn insert(pool: &SqlitePool, row: IntegrationInsert<'_>) -> Result<i64, ApiError> {
    let result = sqlx::query(
        r#"
        INSERT INTO integrations (type, provider, display_name, enabled, config_json, config_secrets_enc, sort_order)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(row.type_.as_str())
    .bind(row.provider.as_str())
    .bind(row.display_name)
    .bind(if row.enabled { 1i64 } else { 0 })
    .bind(row.config_json)
    .bind(row.config_secrets_enc)
    .bind(i64::from(row.sort_order))
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn update(pool: &SqlitePool, id: i64, patch: IntegrationUpdate<'_>) -> Result<(), ApiError> {
    let existing = get_by_id(pool, id)
        .await?
        .ok_or_else(|| ApiError::Message("integration not found".into()))?;

    let display_name = patch.display_name.unwrap_or(&existing.display_name);
    let enabled = patch
        .enabled
        .map(|e| if e { 1i64 } else { 0 })
        .unwrap_or(existing.enabled);
    let config_json = patch.config_json.unwrap_or(&existing.config_json);
    let sort_order = patch
        .sort_order
        .map(i64::from)
        .unwrap_or(existing.sort_order);

    let secrets_enc = match patch.config_secrets_enc {
        None => existing.config_secrets_enc,
        Some(None) => None,
        Some(Some(s)) => Some(s),
    };

    sqlx::query(
        r#"
        UPDATE integrations
        SET display_name = ?, enabled = ?, config_json = ?, config_secrets_enc = ?,
            sort_order = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(display_name)
    .bind(enabled)
    .bind(config_json)
    .bind(secrets_enc)
    .bind(sort_order)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let result = sqlx::query("DELETE FROM integrations WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn max_sort_order(pool: &SqlitePool, type_: IntegrationType) -> Result<i32, ApiError> {
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT MAX(sort_order) FROM integrations WHERE type = ?")
            .bind(type_.as_str())
            .fetch_optional(pool)
            .await?;
    Ok(row
        .and_then(|(m,)| m)
        .map(|n| n as i32 + 1)
        .unwrap_or(0))
}

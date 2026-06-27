use chrono::{DateTime, Utc};
use euterpe_data::DataHandle;
use euterpe_data::repositories::qobuz as data;
use sqlx::SqlitePool;

use crate::error::ApiError;

#[derive(Debug, Clone)]
pub struct QobuzAccountListItem {
    pub id: i64,
    pub label: Option<String>,
    pub qobuz_user_id: i64,
    pub display_name: Option<String>,
    pub membership_label: Option<String>,
    pub uat_obtained_at: String,
    pub uat_expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct QobuzAccountRecord {
    pub id: i64,
    pub qobuz_user_id: i64,
    pub uat_encrypted: String,
    pub display_name: Option<String>,
    pub membership_label: Option<String>,
    pub uat_obtained_at: String,
    pub uat_expires_at: Option<String>,
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<QobuzAccountRecord>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::get_by_id(&handle, id).await?.map(record_from_data))
}

pub async fn find_by_qobuz_user_id(
    pool: &SqlitePool,
    qobuz_user_id: i64,
) -> Result<Option<QobuzAccountRecord>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::find_by_qobuz_user_id(&handle, qobuz_user_id)
        .await?
        .map(record_from_data))
}

pub async fn list_without_uat(pool: &SqlitePool) -> Result<Vec<QobuzAccountListItem>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::list_without_uat(&handle)
        .await?
        .into_iter()
        .map(list_item_from_data)
        .collect())
}

pub async fn upsert_after_oauth(
    pool: &SqlitePool,
    qobuz_user_id: i64,
    uat_encrypted: &str,
    display_name: Option<&str>,
    membership_label: Option<&str>,
    uat_obtained_at: DateTime<Utc>,
    uat_expires_at: Option<DateTime<Utc>>,
) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::upsert_after_oauth(
        &handle,
        qobuz_user_id,
        uat_encrypted,
        display_name,
        membership_label,
        uat_obtained_at,
        uat_expires_at,
    )
    .await?)
}

pub async fn delete_by_id(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::delete_by_id(&handle, id).await?)
}

pub async fn purge_expired_oauth_states(pool: &SqlitePool) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::purge_expired_oauth_states(&handle).await?)
}

pub async fn insert_oauth_state(
    pool: &SqlitePool,
    state: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::insert_oauth_state(&handle, state, expires_at).await?)
}

/// When Qobuz redirects without `state`, accept the flow only if exactly one pending state exists.
pub async fn consume_sole_pending_oauth_state(
    pool: &SqlitePool,
) -> Result<Option<String>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::consume_sole_pending_oauth_state(&handle).await?)
}

/// Deletes the row if it exists and is not expired. Returns whether a valid row was consumed.
pub async fn consume_oauth_state(pool: &SqlitePool, state: &str) -> Result<bool, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data::consume_oauth_state(&handle, state).await?)
}

fn record_from_data(row: data::QobuzAccountRecord) -> QobuzAccountRecord {
    QobuzAccountRecord {
        id: row.id,
        qobuz_user_id: row.qobuz_user_id,
        uat_encrypted: row.uat_encrypted,
        display_name: row.display_name,
        membership_label: row.membership_label,
        uat_obtained_at: row.uat_obtained_at,
        uat_expires_at: row.uat_expires_at,
    }
}

fn list_item_from_data(row: data::QobuzAccountListItem) -> QobuzAccountListItem {
    QobuzAccountListItem {
        id: row.id,
        label: row.label,
        qobuz_user_id: row.qobuz_user_id,
        display_name: row.display_name,
        membership_label: row.membership_label,
        uat_obtained_at: row.uat_obtained_at,
        uat_expires_at: row.uat_expires_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

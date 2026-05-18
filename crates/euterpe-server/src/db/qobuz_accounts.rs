use chrono::{DateTime, Utc};
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
    let row = sqlx::query_as::<_, (i64, i64, String, Option<String>, Option<String>, String, Option<String>)>(
        r#"
        SELECT id, qobuz_user_id, uat_encrypted, display_name, membership_label, uat_obtained_at, uat_expires_at
        FROM qobuz_accounts WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(id, qobuz_user_id, uat_encrypted, display_name, membership_label, uat_obtained_at, uat_expires_at)| {
            QobuzAccountRecord {
                id,
                qobuz_user_id,
                uat_encrypted,
                display_name,
                membership_label,
                uat_obtained_at,
                uat_expires_at,
            }
        },
    ))
}

pub async fn find_by_qobuz_user_id(
    pool: &SqlitePool,
    qobuz_user_id: i64,
) -> Result<Option<QobuzAccountRecord>, ApiError> {
    let row = sqlx::query_as::<_, (i64, i64, String, Option<String>, Option<String>, String, Option<String>)>(
        r#"
        SELECT id, qobuz_user_id, uat_encrypted, display_name, membership_label, uat_obtained_at, uat_expires_at
        FROM qobuz_accounts WHERE qobuz_user_id = ?
        "#,
    )
    .bind(qobuz_user_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(id, qobuz_user_id, uat_encrypted, display_name, membership_label, uat_obtained_at, uat_expires_at)| {
            QobuzAccountRecord {
                id,
                qobuz_user_id,
                uat_encrypted,
                display_name,
                membership_label,
                uat_obtained_at,
                uat_expires_at,
            }
        },
    ))
}

pub async fn list_without_uat(pool: &SqlitePool) -> Result<Vec<QobuzAccountListItem>, ApiError> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            Option<String>,
            i64,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
            String,
        ),
    >(
        r#"
        SELECT id, label, qobuz_user_id, display_name, membership_label, uat_obtained_at, uat_expires_at, created_at, updated_at
        FROM qobuz_accounts ORDER BY id ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                label,
                qobuz_user_id,
                display_name,
                membership_label,
                uat_obtained_at,
                uat_expires_at,
                created_at,
                updated_at,
            )| {
                QobuzAccountListItem {
                    id,
                    label,
                    qobuz_user_id,
                    display_name,
                    membership_label,
                    uat_obtained_at,
                    uat_expires_at,
                    created_at,
                    updated_at,
                }
            },
        )
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
    let obtained = uat_obtained_at.to_rfc3339();
    let expires = uat_expires_at.map(|t| t.to_rfc3339());
    let existing = find_by_qobuz_user_id(pool, qobuz_user_id).await?;
    if let Some(row) = existing {
        sqlx::query(
            r#"
            UPDATE qobuz_accounts SET
                uat_encrypted = ?,
                display_name = ?,
                membership_label = ?,
                uat_obtained_at = ?,
                uat_expires_at = ?,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(uat_encrypted)
        .bind(display_name)
        .bind(membership_label)
        .bind(&obtained)
        .bind(expires.as_deref())
        .bind(row.id)
        .execute(pool)
        .await?;
        return Ok(row.id);
    }

    let res = sqlx::query(
        r#"
        INSERT INTO qobuz_accounts (
            qobuz_user_id, uat_encrypted, display_name, membership_label, uat_obtained_at, uat_expires_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(qobuz_user_id)
    .bind(uat_encrypted)
    .bind(display_name)
    .bind(membership_label)
    .bind(&obtained)
    .bind(expires.as_deref())
    .execute(pool)
    .await?;

    Ok(res.last_insert_rowid() as i64)
}

pub async fn delete_by_id(pool: &SqlitePool, id: i64) -> Result<bool, ApiError> {
    let n = sqlx::query("DELETE FROM qobuz_accounts WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(n > 0)
}

pub async fn purge_expired_oauth_states(pool: &SqlitePool) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM qobuz_oauth_states WHERE datetime(expires_at) < datetime('now')")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_oauth_state(
    pool: &SqlitePool,
    state: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO qobuz_oauth_states (state, expires_at) VALUES (?, ?)
        "#,
    )
    .bind(state)
    .bind(expires_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// When Qobuz redirects without `state`, accept the flow only if exactly one pending state exists.
pub async fn consume_sole_pending_oauth_state(pool: &SqlitePool) -> Result<Option<String>, ApiError> {
    let rows: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT state FROM qobuz_oauth_states
        WHERE datetime(expires_at) >= datetime('now')
        "#,
    )
    .fetch_all(pool)
    .await?;

    if rows.len() != 1 {
        return Ok(None);
    }
    let state = rows[0].clone();
    if consume_oauth_state(pool, &state).await? {
        Ok(Some(state))
    } else {
        Ok(None)
    }
}

/// Deletes the row if it exists and is not expired. Returns whether a valid row was consumed.
pub async fn consume_oauth_state(pool: &SqlitePool, state: &str) -> Result<bool, ApiError> {
    let n = sqlx::query(
        r#"
        DELETE FROM qobuz_oauth_states
        WHERE state = ? AND datetime(expires_at) >= datetime('now')
        "#,
    )
    .bind(state)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n > 0)
}

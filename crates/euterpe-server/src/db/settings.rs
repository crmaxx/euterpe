use sqlx::SqlitePool;

use crate::error::ApiError;

pub const KEY_QOBUZ_USER_ID: &str = "qobuz.user_id";
pub const KEY_QOBUZ_UAT_ENC: &str = "qobuz.uat_enc";

pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>, ApiError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES (?, ?, datetime('now'))
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = datetime('now')
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

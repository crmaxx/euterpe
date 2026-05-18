use sqlx::SqlitePool;

use crate::error::ApiError;

pub async fn upsert_by_name(
    pool: &SqlitePool,
    name: &str,
    qobuz_artist_id: Option<i64>,
) -> Result<i64, ApiError> {
    if let Some(qid) = qobuz_artist_id {
        let existing: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM artists WHERE qobuz_artist_id = ?")
                .bind(qid)
                .fetch_optional(pool)
                .await?;
        if let Some((id,)) = existing {
            sqlx::query("UPDATE artists SET name = ? WHERE id = ?")
                .bind(name)
                .bind(id)
                .execute(pool)
                .await?;
            return Ok(id);
        }
    }

    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM artists WHERE name = ? COLLATE NOCASE AND qobuz_artist_id IS NULL",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    if let Some((id,)) = existing {
        if qobuz_artist_id.is_some() {
            sqlx::query("UPDATE artists SET qobuz_artist_id = ? WHERE id = ?")
                .bind(qobuz_artist_id)
                .bind(id)
                .execute(pool)
                .await?;
        }
        return Ok(id);
    }

    let result = sqlx::query("INSERT INTO artists (name, qobuz_artist_id) VALUES (?, ?)")
        .bind(name)
        .bind(qobuz_artist_id)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connect, migrate};

    #[tokio::test]
    async fn upsert_artist_returns_stable_id() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let a = upsert_by_name(&pool, "Artist A", None).await.unwrap();
        let b = upsert_by_name(&pool, "Artist A", None).await.unwrap();
        assert_eq!(a, b);
    }
}

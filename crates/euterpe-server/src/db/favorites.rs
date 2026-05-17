use chrono::Utc;
use sqlx::SqlitePool;

use crate::api::QobuzFavoriteItem;
use crate::error::ApiError;

#[derive(Debug, sqlx::FromRow)]
struct FavoriteRow {
    qobuz_id: i64,
    album_api_id: Option<String>,
    title: Option<String>,
    artist_name: Option<String>,
}

/// `album_api_id` is stored in `slug` column: short ref, long slug, or catalog id for `album/get`.
pub async fn upsert_album(
    pool: &SqlitePool,
    qobuz_id: u64,
    title: &str,
    artist_name: &str,
    album_api_id: Option<&str>,
) -> Result<bool, ApiError> {
    let synced_at = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        INSERT INTO qobuz_favorites (entity_type, qobuz_id, title, artist_name, slug, synced_at, removed)
        VALUES ('album', ?, ?, ?, ?, ?, 0)
        ON CONFLICT(entity_type, qobuz_id) DO UPDATE SET
            title = excluded.title,
            artist_name = excluded.artist_name,
            slug = excluded.slug,
            synced_at = excluded.synced_at,
            removed = 0
        "#,
    )
    .bind(qobuz_id as i64)
    .bind(title)
    .bind(artist_name)
    .bind(album_api_id)
    .bind(&synced_at)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[derive(Debug, Clone)]
pub struct FavoriteAlbumMeta {
    pub slug: Option<String>,
    pub title: String,
    pub artist_name: String,
}

pub async fn album_meta(
    pool: &SqlitePool,
    qobuz_id: u64,
) -> Result<Option<FavoriteAlbumMeta>, ApiError> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT slug, title, artist_name
        FROM qobuz_favorites
        WHERE entity_type = 'album' AND qobuz_id = ? AND removed = 0
        "#,
    )
    .bind(qobuz_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(slug, title, artist)| FavoriteAlbumMeta {
        slug: slug.filter(|s| !s.trim().is_empty()),
        title: title.unwrap_or_default(),
        artist_name: artist.unwrap_or_default(),
    }))
}

pub async fn mark_removed_except(
    pool: &SqlitePool,
    keep_ids: &[u64],
) -> Result<u64, ApiError> {
    if keep_ids.is_empty() {
        let result = sqlx::query(
            "UPDATE qobuz_favorites SET removed = 1 WHERE entity_type = 'album' AND removed = 0",
        )
        .execute(pool)
        .await?;
        return Ok(result.rows_affected() as u64);
    }

    let placeholders = keep_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "UPDATE qobuz_favorites SET removed = 1 WHERE entity_type = 'album' AND removed = 0 AND qobuz_id NOT IN ({placeholders})"
    );
    let mut query = sqlx::query(&sql);
    for id in keep_ids {
        query = query.bind(*id as i64);
    }
    let result = query.execute(pool).await?;
    Ok(result.rows_affected() as u64)
}

pub async fn list_albums(
    pool: &SqlitePool,
    page: u32,
    limit: u32,
) -> Result<(Vec<QobuzFavoriteItem>, i64), ApiError> {
    let offset = (page as i64) * (limit as i64);
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM qobuz_favorites WHERE entity_type = 'album' AND removed = 0",
    )
    .fetch_one(pool)
    .await?;

    let rows: Vec<FavoriteRow> = sqlx::query_as(
        r#"
        SELECT qobuz_id, slug AS album_api_id, title, artist_name
        FROM qobuz_favorites
        WHERE entity_type = 'album' AND removed = 0
        ORDER BY title COLLATE NOCASE
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit as i64)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|r| QobuzFavoriteItem {
            album_api_id: r
                .album_api_id
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| r.qobuz_id.to_string()),
            qobuz_id: r.qobuz_id,
            title: r.title.unwrap_or_default(),
            artist_name: r.artist_name.unwrap_or_default(),
            in_library: false,
            local_album_id: None,
        })
        .collect();

    Ok((items, total.0))
}

pub async fn mark_albums_removed(pool: &SqlitePool, ids: &[u64]) -> Result<(), ApiError> {
    for id in ids {
        sqlx::query(
            "UPDATE qobuz_favorites SET removed = 1 WHERE entity_type = 'album' AND qobuz_id = ?",
        )
        .bind(*id as i64)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn active_album_ids(pool: &SqlitePool) -> Result<Vec<u64>, ApiError> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT qobuz_id FROM qobuz_favorites WHERE entity_type = 'album' AND removed = 0",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id as u64).collect())
}

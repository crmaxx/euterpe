use sqlx::SqlitePool;

use crate::error::ApiError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AlbumRow {
    pub id: i64,
    pub artist_id: Option<i64>,
    pub title: String,
    pub year: Option<i32>,
    pub qobuz_album_id: Option<i64>,
    pub path: Option<String>,
    pub cover_path: Option<String>,
}

pub struct AlbumUpsert<'a> {
    pub artist_id: Option<i64>,
    pub title: &'a str,
    pub year: Option<i32>,
    pub qobuz_album_id: Option<i64>,
    pub path: Option<&'a str>,
    pub cover_path: Option<&'a str>,
}

pub async fn upsert(pool: &SqlitePool, album: AlbumUpsert<'_>) -> Result<i64, ApiError> {
    if let Some(path) = album.path {
        let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM albums WHERE path = ?")
            .bind(path)
            .fetch_optional(pool)
            .await?;
        if let Some((id,)) = existing {
            sqlx::query(
                r#"
                UPDATE albums
                SET artist_id = ?, title = ?, year = ?, qobuz_album_id = COALESCE(?, qobuz_album_id),
                    cover_path = COALESCE(?, cover_path), updated_at = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(album.artist_id)
            .bind(album.title)
            .bind(album.year)
            .bind(album.qobuz_album_id)
            .bind(album.cover_path)
            .bind(id)
            .execute(pool)
            .await?;
            return Ok(id);
        }
    }

    if let Some(qid) = album.qobuz_album_id {
        let existing: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM albums WHERE qobuz_album_id = ?")
                .bind(qid)
                .fetch_optional(pool)
                .await?;
        if let Some((id,)) = existing {
            sqlx::query(
                r#"
                UPDATE albums
                SET artist_id = ?, title = ?, year = ?, path = COALESCE(?, path),
                    cover_path = COALESCE(?, cover_path), updated_at = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(album.artist_id)
            .bind(album.title)
            .bind(album.year)
            .bind(album.path)
            .bind(album.cover_path)
            .bind(id)
            .execute(pool)
            .await?;
            return Ok(id);
        }
    }

    let result = sqlx::query(
        r#"
        INSERT INTO albums (artist_id, title, year, qobuz_album_id, path, cover_path)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(album.artist_id)
    .bind(album.title)
    .bind(album.year)
    .bind(album.qobuz_album_id)
    .bind(album.path)
    .bind(album.cover_path)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<AlbumRow>, ApiError> {
    let row: Option<AlbumRow> = sqlx::query_as(
        r#"
        SELECT id, artist_id, title, year, qobuz_album_id, path, cover_path
        FROM albums WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list(
    pool: &SqlitePool,
    page: u32,
    limit: u32,
    search: Option<&str>,
) -> Result<(Vec<AlbumListRow>, i64), ApiError> {
    let offset = (page as i64) * (limit as i64);
    let pattern = search.map(|s| format!("%{s}%"));

    let total: (i64,) = if let Some(ref p) = pattern {
        sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM albums a
            LEFT JOIN artists ar ON a.artist_id = ar.id
            WHERE a.title LIKE ? OR ar.name LIKE ?
            "#,
        )
        .bind(p)
        .bind(p)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_as("SELECT COUNT(*) FROM albums")
            .fetch_one(pool)
            .await?
    };

    let rows: Vec<AlbumListRow> = if let Some(ref p) = pattern {
        sqlx::query_as(
            r#"
            SELECT
                a.id,
                a.title,
                COALESCE(ar.name, '') AS artist_name,
                a.year,
                a.cover_path,
                (SELECT COUNT(*) FROM tracks t WHERE t.album_id = a.id) AS track_count
            FROM albums a
            LEFT JOIN artists ar ON a.artist_id = ar.id
            WHERE a.title LIKE ? OR ar.name LIKE ?
            ORDER BY a.title COLLATE NOCASE
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(p)
        .bind(p)
        .bind(limit as i64)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            r#"
            SELECT
                a.id,
                a.title,
                COALESCE(ar.name, '') AS artist_name,
                a.year,
                a.cover_path,
                (SELECT COUNT(*) FROM tracks t WHERE t.album_id = a.id) AS track_count
            FROM albums a
            LEFT JOIN artists ar ON a.artist_id = ar.id
            ORDER BY a.title COLLATE NOCASE
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit as i64)
        .bind(offset)
        .fetch_all(pool)
        .await?
    };

    Ok((rows, total.0))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AlbumListRow {
    pub id: i64,
    pub title: String,
    pub artist_name: String,
    pub year: Option<i32>,
    pub cover_path: Option<String>,
    pub track_count: i64,
}

pub async fn set_cover_path(pool: &SqlitePool, id: i64, cover_path: &str) -> Result<(), ApiError> {
    let n = sqlx::query(
        "UPDATE albums SET cover_path = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(cover_path)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::Message("album not found".into()));
    }
    Ok(())
}

pub async fn find_id_by_qobuz_album_id(
    pool: &SqlitePool,
    qobuz_id: i64,
) -> Result<Option<i64>, ApiError> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM albums WHERE qobuz_album_id = ?")
            .bind(qobuz_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(id,)| id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{artists, connect, migrate};

    #[tokio::test]
    async fn upsert_album_by_path() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let artist_id = artists::upsert_by_name(&pool, "A", None).await.unwrap();
        let id1 = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album",
                year: Some(2020),
                qobuz_album_id: None,
                path: Some("/music/A/Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let id2 = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album Updated",
                year: Some(2021),
                qobuz_album_id: None,
                path: Some("/music/A/Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(id1, id2);
        let row = get_by_id(&pool, id1).await.unwrap().unwrap();
        assert_eq!(row.title, "Album Updated");
    }
}

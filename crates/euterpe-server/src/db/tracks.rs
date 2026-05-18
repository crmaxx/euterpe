use sqlx::SqlitePool;

use crate::error::ApiError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TrackRow {
    pub id: i64,
    pub album_id: i64,
    pub title: String,
    pub track_number: Option<i32>,
    pub year: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<String>,
    pub qobuz_track_id: Option<i64>,
    pub path: String,
    pub duration_sec: Option<i32>,
    pub file_mtime: Option<String>,
    pub file_hash: Option<String>,
}

pub struct TrackUpsert<'a> {
    pub album_id: i64,
    pub title: &'a str,
    pub track_number: Option<i32>,
    pub year: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<&'a str>,
    pub qobuz_track_id: Option<i64>,
    pub path: &'a str,
    pub duration_sec: Option<i32>,
    pub file_mtime: Option<&'a str>,
    pub file_hash: Option<&'a str>,
}

pub async fn upsert(pool: &SqlitePool, track: TrackUpsert<'_>) -> Result<i64, ApiError> {
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM tracks WHERE path = ?")
        .bind(track.path)
        .fetch_optional(pool)
        .await?;

    if let Some((id,)) = existing {
        sqlx::query(
            r#"
            UPDATE tracks
            SET album_id = ?, title = ?, track_number = ?, year = ?, disc_number = ?, genre = ?,
                qobuz_track_id = COALESCE(?, qobuz_track_id),
                duration_sec = ?, file_mtime = ?, file_hash = ?,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(track.album_id)
        .bind(track.title)
        .bind(track.track_number)
        .bind(track.year)
        .bind(track.disc_number)
        .bind(track.genre)
        .bind(track.qobuz_track_id)
        .bind(track.duration_sec)
        .bind(track.file_mtime)
        .bind(track.file_hash)
        .bind(id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    let result = sqlx::query(
        r#"
        INSERT INTO tracks (
            album_id, title, track_number, year, disc_number, genre, qobuz_track_id, path,
            duration_sec, file_mtime, file_hash
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(track.album_id)
    .bind(track.title)
    .bind(track.track_number)
    .bind(track.year)
    .bind(track.disc_number)
    .bind(track.genre)
    .bind(track.qobuz_track_id)
    .bind(track.path)
    .bind(track.duration_sec)
    .bind(track.file_mtime)
    .bind(track.file_hash)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<TrackRow>, ApiError> {
    let row: Option<TrackRow> = sqlx::query_as(
        r#"
        SELECT id, album_id, title, track_number, year, disc_number, genre, qobuz_track_id, path,
               duration_sec, file_mtime, file_hash
        FROM tracks WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list_by_album(pool: &SqlitePool, album_id: i64) -> Result<Vec<TrackRow>, ApiError> {
    let rows: Vec<TrackRow> = sqlx::query_as(
        r#"
        SELECT id, album_id, title, track_number, year, disc_number, genre, qobuz_track_id, path,
               duration_sec, file_mtime, file_hash
        FROM tracks
        WHERE album_id = ?
        ORDER BY COALESCE(disc_number, 1), COALESCE(track_number, 999999), title
        "#,
    )
    .bind(album_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_metadata(
    pool: &SqlitePool,
    id: i64,
    title: &str,
    track_number: Option<i32>,
    year: Option<i32>,
    disc_number: Option<i32>,
    genre: Option<&str>,
    file_mtime: Option<&str>,
) -> Result<(), ApiError> {
    let n = sqlx::query(
        r#"
        UPDATE tracks
        SET title = ?, track_number = ?, year = ?, disc_number = ?, genre = ?,
            file_mtime = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(title)
    .bind(track_number)
    .bind(year)
    .bind(disc_number)
    .bind(genre)
    .bind(file_mtime)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::Message("track not found".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{albums, artists, connect, migrate};

    #[tokio::test]
    async fn upsert_track_by_path() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let artist_id = artists::upsert_by_name(&pool, "A", None).await.unwrap();
        let album_id = albums::upsert(
            &pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Al",
                year: None,
                qobuz_album_id: None,
                path: Some("/music/A/Al"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let id1 = upsert(
            &pool,
            TrackUpsert {
                album_id,
                title: "T1",
                track_number: Some(1),
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: "/music/A/Al/01.flac",
                duration_sec: Some(200),
                file_mtime: None,
                file_hash: None,
            },
        )
        .await
        .unwrap();
        let id2 = upsert(
            &pool,
            TrackUpsert {
                album_id,
                title: "T1 Renamed",
                track_number: Some(1),
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: "/music/A/Al/01.flac",
                duration_sec: Some(201),
                file_mtime: None,
                file_hash: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(id1, id2);
        let row = get_by_id(&pool, id1).await.unwrap().unwrap();
        assert_eq!(row.title, "T1 Renamed");
    }
}

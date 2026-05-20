use std::path::Path;

use sqlx::SqlitePool;

use crate::error::ApiError;

fn filename_sort_key(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| path.to_lowercase())
}

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
    pub file_size: Option<i64>,
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
    pub file_size: Option<i64>,
}

/// Stored mtime + size for skip-unchanged during library scan.
pub async fn get_fingerprint_by_path(
    pool: &SqlitePool,
    path: &str,
) -> Result<Option<(Option<String>, Option<i64>)>, ApiError> {
    let row: Option<(Option<String>, Option<i64>)> =
        sqlx::query_as("SELECT file_mtime, file_size FROM tracks WHERE path = ?")
            .bind(path)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

/// Fields updated by `update_metadata` (library tag PATCH → DB row).
pub struct TrackMetadataUpdate<'a> {
    pub title: &'a str,
    pub track_number: Option<i32>,
    pub year: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<&'a str>,
    pub file_mtime: Option<&'a str>,
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
                duration_sec = ?, file_mtime = ?, file_hash = ?, file_size = ?,
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
        .bind(track.file_size)
        .bind(id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    let result = sqlx::query(
        r#"
        INSERT INTO tracks (
            album_id, title, track_number, year, disc_number, genre, qobuz_track_id, path,
            duration_sec, file_mtime, file_hash, file_size
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    .bind(track.file_size)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<TrackRow>, ApiError> {
    let row: Option<TrackRow> = sqlx::query_as(
        r#"
        SELECT id, album_id, title, track_number, year, disc_number, genre, qobuz_track_id, path,
               duration_sec, file_mtime, file_hash, file_size
        FROM tracks WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list_by_album(pool: &SqlitePool, album_id: i64) -> Result<Vec<TrackRow>, ApiError> {
    let mut rows: Vec<TrackRow> = sqlx::query_as(
        r#"
        SELECT id, album_id, title, track_number, year, disc_number, genre, qobuz_track_id, path,
               duration_sec, file_mtime, file_hash, file_size
        FROM tracks
        WHERE album_id = ?
        "#,
    )
    .bind(album_id)
    .fetch_all(pool)
    .await?;
    rows.sort_by_key(|a| filename_sort_key(&a.path));
    Ok(rows)
}

pub async fn update_metadata(
    pool: &SqlitePool,
    id: i64,
    meta: TrackMetadataUpdate<'_>,
) -> Result<(), ApiError> {
    let n = sqlx::query(
        r#"
        UPDATE tracks
        SET title = ?, track_number = ?, year = ?, disc_number = ?, genre = ?,
            file_mtime = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(meta.title)
    .bind(meta.track_number)
    .bind(meta.year)
    .bind(meta.disc_number)
    .bind(meta.genre)
    .bind(meta.file_mtime)
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
                file_size: None,
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
                file_size: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(id1, id2);
        let row = get_by_id(&pool, id1).await.unwrap().unwrap();
        assert_eq!(row.title, "T1 Renamed");
    }

    #[tokio::test]
    async fn list_by_album_sorted_by_filename_asc() {
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
                path: Some("A/Al"),
                cover_path: None,
            },
        )
        .await
        .unwrap();

        for (path, title) in [
            ("A/Al/10 - Ten.flac", "Ten"),
            ("A/Al/02 - Two.flac", "Two"),
            ("A/Al/01 - One.flac", "One"),
        ] {
            upsert(
                &pool,
                TrackUpsert {
                    album_id,
                    title,
                    track_number: None,
                    year: None,
                    disc_number: None,
                    genre: None,
                    qobuz_track_id: None,
                    path,
                    duration_sec: None,
                    file_mtime: None,
                    file_hash: None,
                    file_size: None,
                },
            )
            .await
            .unwrap();
        }

        let listed = list_by_album(&pool, album_id).await.unwrap();
        let paths: Vec<_> = listed.iter().map(|t| t.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "A/Al/01 - One.flac",
                "A/Al/02 - Two.flac",
                "A/Al/10 - Ten.flac",
            ]
        );
    }
}

use sqlx::SqlitePool;

use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::catalog;

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
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::get_track_fingerprint_by_path(&handle, path).await?)
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
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::upsert_track(
        &handle,
        catalog::TrackUpsert {
            album_id: track.album_id,
            title: track.title,
            track_number: track.track_number,
            year: track.year,
            disc_number: track.disc_number,
            genre: track.genre,
            qobuz_track_id: track.qobuz_track_id,
            path: track.path,
            duration_sec: track.duration_sec,
            file_mtime: track.file_mtime,
            file_hash: track.file_hash,
            file_size: track.file_size,
        },
    )
    .await?)
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<TrackRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::get_track_by_id(&handle, id)
        .await?
        .map(track_row_from_data))
}

pub async fn list_by_album(pool: &SqlitePool, album_id: i64) -> Result<Vec<TrackRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::list_tracks_by_album(&handle, album_id)
        .await?
        .into_iter()
        .map(track_row_from_data)
        .collect())
}

pub async fn list_by_album_or_path_prefix(
    pool: &SqlitePool,
    album_id: i64,
    album_path: Option<&str>,
) -> Result<Vec<TrackRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(
        catalog::list_tracks_by_album_or_path_prefix(&handle, album_id, album_path)
            .await?
            .into_iter()
            .map(track_row_from_data)
            .collect(),
    )
}

pub async fn update_metadata(
    pool: &SqlitePool,
    id: i64,
    meta: TrackMetadataUpdate<'_>,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    if !catalog::update_track_metadata(
        &handle,
        id,
        catalog::TrackMetadataUpdate {
            title: meta.title,
            track_number: meta.track_number,
            year: meta.year,
            disc_number: meta.disc_number,
            genre: meta.genre,
            file_mtime: meta.file_mtime,
        },
    )
    .await?
    {
        return Err(ApiError::Message("track not found".into()));
    }
    Ok(())
}

pub async fn update_path(pool: &SqlitePool, id: i64, path: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    if !catalog::update_track_path(&handle, id, path).await? {
        return Err(ApiError::Message("track not found".into()));
    }
    Ok(())
}

pub async fn delete_by_path(pool: &SqlitePool, path: &str) -> Result<u64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::delete_track_by_path(&handle, path).await?)
}

pub async fn delete_by_path_or_prefix(pool: &SqlitePool, path: &str) -> Result<u64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::delete_tracks_by_path_or_prefix(&handle, path).await?)
}

pub async fn delete_absent_in_scope(
    pool: &SqlitePool,
    scope_path: Option<&str>,
    keep_paths: &[String],
) -> Result<u64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::delete_absent_in_scope(&handle, scope_path, keep_paths).await?)
}

pub async fn reset_scan_keep_paths(pool: &SqlitePool, scan_id: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    catalog::reset_scan_keep_paths(&handle, scan_id).await?;
    Ok(())
}

pub async fn record_scan_keep_path(
    pool: &SqlitePool,
    scan_id: i64,
    path: &str,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    catalog::record_scan_keep_path(&handle, scan_id, path).await?;
    Ok(())
}

pub async fn delete_absent_in_scope_for_scan(
    pool: &SqlitePool,
    scope_path: Option<&str>,
    scan_id: i64,
) -> Result<u64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::delete_absent_in_scope_for_scan(&handle, scope_path, scan_id).await?)
}

pub async fn cleanup_scan_keep_paths(pool: &SqlitePool, scan_id: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    catalog::cleanup_scan_keep_paths(&handle, scan_id).await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TrackHashBackfillRow {
    pub id: i64,
    pub path: String,
    pub file_size: Option<i64>,
}

pub async fn list_needing_file_hash_batch(
    pool: &SqlitePool,
    after_id: i64,
    limit: i64,
) -> Result<Vec<TrackHashBackfillRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(
        catalog::list_tracks_needing_file_hash_batch(&handle, after_id, limit)
            .await?
            .into_iter()
            .map(|row| TrackHashBackfillRow {
                id: row.id,
                path: row.path,
                file_size: row.file_size,
            })
            .collect(),
    )
}

pub async fn set_file_hash(pool: &SqlitePool, id: i64, file_hash: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    if !catalog::set_track_file_hash(&handle, id, file_hash).await? {
        return Err(ApiError::Message("track not found".into()));
    }
    Ok(())
}

pub async fn set_file_size(pool: &SqlitePool, id: i64, file_size: i64) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    if !catalog::set_track_file_size(&handle, id, file_size).await? {
        return Err(ApiError::Message("track not found".into()));
    }
    Ok(())
}

pub async fn count(pool: &SqlitePool) -> Result<usize, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::count_tracks(&handle).await?)
}

pub async fn count_distinct_paths(pool: &SqlitePool) -> Result<usize, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::count_distinct_track_paths(&handle).await?)
}

pub async fn update_path_fingerprint(
    pool: &SqlitePool,
    id: i64,
    path: &str,
    file_size: Option<i64>,
    file_hash: Option<&str>,
    file_mtime: Option<&str>,
) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    if !catalog::update_track_path_fingerprint(&handle, id, path, file_size, file_hash, file_mtime)
        .await?
    {
        return Err(ApiError::Message("track not found".into()));
    }
    Ok(())
}

fn track_row_from_data(row: catalog::TrackRow) -> TrackRow {
    TrackRow {
        id: row.id,
        album_id: row.album_id,
        title: row.title,
        track_number: row.track_number,
        year: row.year,
        disc_number: row.disc_number,
        genre: row.genre,
        qobuz_track_id: row.qobuz_track_id,
        path: row.path,
        duration_sec: row.duration_sec,
        file_mtime: row.file_mtime,
        file_hash: row.file_hash,
        file_size: row.file_size,
    }
}

pub fn path_extension_lower(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

pub async fn album_has_convertible_tracks(
    pool: &SqlitePool,
    album_id: i64,
) -> Result<bool, ApiError> {
    let rows = list_by_album(pool, album_id).await?;
    Ok(rows
        .iter()
        .any(|t| crate::library::tags::is_convertible_path(std::path::Path::new(&t.path))))
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

    #[tokio::test]
    async fn delete_by_path_or_prefix_keeps_unrelated_tracks() {
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

        for path in ["A/Al/01.flac", "A/Al/Disc 2/02.flac", "A/Other/01.flac"] {
            upsert(
                &pool,
                TrackUpsert {
                    album_id,
                    title: path,
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

        let deleted = delete_by_path_or_prefix(&pool, "A/Al").await.unwrap();
        assert_eq!(deleted, 2);
        let rows = list_by_album(&pool, album_id).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "A/Other/01.flac");
    }

    #[tokio::test]
    async fn delete_by_path_or_prefix_keeps_sibling_with_same_prefix_text() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let artist_id = artists::upsert_by_name(&pool, "Artist", None)
            .await
            .unwrap();
        let album_id = albums::upsert(
            &pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album",
                year: None,
                qobuz_album_id: None,
                path: Some("Artist/Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap();

        for path in [
            "Artist/Album/01.flac",
            "Artist/Album/02.flac",
            "Artist/AlbumX/01.flac",
        ] {
            upsert(
                &pool,
                TrackUpsert {
                    album_id,
                    title: path,
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

        let deleted = delete_by_path_or_prefix(&pool, "Artist/Album")
            .await
            .unwrap();
        assert_eq!(deleted, 2);
        let rows = list_by_album(&pool, album_id).await.unwrap();
        let paths: Vec<_> = rows.iter().map(|row| row.path.as_str()).collect();
        assert_eq!(paths, ["Artist/AlbumX/01.flac"]);
    }

    #[test]
    fn path_prefix_helpers_do_not_reintroduce_substr_predicates() {
        let source = include_str!("tracks.rs");
        let needle = ["substr", "(path"].join("");
        assert!(
            !source.contains(&needle),
            "path prefix helpers should use exact-or-range predicates so idx_tracks_path remains usable"
        );
    }

    #[tokio::test]
    async fn list_needing_file_hash_batch_pages_by_id() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let artist_id = artists::upsert_by_name(&pool, "A", None).await.unwrap();
        let album_id = albums::upsert(
            &pool,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album",
                year: None,
                qobuz_album_id: None,
                path: Some("A/Album"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        for n in 1..=3 {
            upsert(
                &pool,
                TrackUpsert {
                    album_id,
                    title: &format!("Track {n}"),
                    track_number: Some(n),
                    year: None,
                    disc_number: None,
                    genre: None,
                    qobuz_track_id: None,
                    path: &format!("A/Album/{n:02}.flac"),
                    duration_sec: None,
                    file_mtime: None,
                    file_hash: None,
                    file_size: Some(10),
                },
            )
            .await
            .unwrap();
        }

        let first = list_needing_file_hash_batch(&pool, 0, 2).await.unwrap();
        assert_eq!(first.len(), 2);
        let second = list_needing_file_hash_batch(&pool, first[1].id, 2)
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert!(second[0].id > first[1].id);
    }

    #[tokio::test]
    async fn delete_absent_in_scope_prunes_only_missing_paths_inside_scope() {
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

        for path in [
            "A/Al/01.flac",
            "A/Al/stale.flac",
            "A/AlbumX/stale.flac",
            "B/Al/stale.flac",
        ] {
            upsert(
                &pool,
                TrackUpsert {
                    album_id,
                    title: path,
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

        let keep = vec!["A/Al/01.flac".to_string()];
        let deleted = delete_absent_in_scope(&pool, Some("A/Al"), &keep)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        let rows = list_by_album(&pool, album_id).await.unwrap();
        let paths: Vec<_> = rows.into_iter().map(|row| row.path).collect();
        assert_eq!(
            paths,
            vec!["A/Al/01.flac", "A/AlbumX/stale.flac", "B/Al/stale.flac"]
        );
    }

    #[tokio::test]
    async fn delete_absent_in_scope_handles_large_keep_sets_without_unbounded_sql_variables() {
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

        upsert(
            &pool,
            TrackUpsert {
                album_id,
                title: "stale",
                track_number: None,
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: "A/Al/stale.flac",
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();

        let keep = (0..40_000)
            .map(|n| format!("A/Al/{n:05}.flac"))
            .collect::<Vec<_>>();

        let deleted = delete_absent_in_scope(&pool, Some("A/Al"), &keep)
            .await
            .unwrap();

        assert_eq!(deleted, 1);
    }

    #[tokio::test]
    async fn delete_absent_in_scope_for_scan_uses_recorded_keep_paths_and_cleans_them() {
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

        for path in ["A/Al/01.flac", "A/Al/stale.flac", "A/AlbumX/stale.flac"] {
            upsert(
                &pool,
                TrackUpsert {
                    album_id,
                    title: path,
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

        reset_scan_keep_paths(&pool, 42).await.unwrap();
        record_scan_keep_path(&pool, 42, "A/Al/01.flac")
            .await
            .unwrap();
        let deleted = delete_absent_in_scope_for_scan(&pool, Some("A/Al"), 42)
            .await
            .unwrap();

        assert_eq!(deleted, 1);
        cleanup_scan_keep_paths(&pool, 42).await.unwrap();
        let handle = DataHandle::from_sqlite_pool(pool.clone());
        assert_eq!(catalog::scan_keep_path_count(&handle, 42).await.unwrap(), 0);
        let rows = list_by_album(&pool, album_id).await.unwrap();
        let paths: Vec<_> = rows.into_iter().map(|row| row.path).collect();
        assert_eq!(paths, vec!["A/Al/01.flac", "A/AlbumX/stale.flac"]);
    }
}

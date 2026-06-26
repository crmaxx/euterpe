use serde_json::json;
use sqlx::SqlitePool;

use crate::api::keyset::{
    decode_cursor, ensure_cursor_matches, fingerprint_json, finish_keyset_page, keyset_and_clause,
};
use crate::api::{KeysetPage, SortKeyKind, SortKeyValue, SortOrder};
use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::catalog;

fn bind_sort_keys<'q, T>(
    mut query: sqlx::query::QueryAs<'q, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments<'q>>,
    binds: &'q [SortKeyValue],
) -> sqlx::query::QueryAs<'q, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments<'q>> {
    for b in binds {
        query = match b {
            SortKeyValue::Text(s) => query.bind(s),
            SortKeyValue::Int(n) => query.bind(n),
            SortKeyValue::Bool(n) => query.bind(n),
        };
    }
    query
}

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
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::upsert_album(
        &handle,
        catalog::AlbumUpsert {
            artist_id: album.artist_id,
            title: album.title,
            year: album.year,
            qobuz_album_id: album.qobuz_album_id,
            path: album.path,
            cover_path: album.cover_path,
        },
    )
    .await?)
}

pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<AlbumRow>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::get_album_by_id(&handle, id)
        .await?
        .map(album_row_from_data))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumsSort {
    Title,
    Artist,
    Year,
}

impl AlbumsSort {
    pub fn parse(s: &str) -> Result<Self, ApiError> {
        match s {
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            "year" => Ok(Self::Year),
            _ => Err(ApiError::bad_request("sort must be title, artist, or year")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Year => "year",
        }
    }

    fn sort_sql(self) -> &'static str {
        match self {
            Self::Title => "a.title COLLATE NOCASE",
            Self::Artist => "COALESCE(ar.name, '') COLLATE NOCASE",
            Self::Year => "COALESCE(a.year, -1)",
        }
    }

    fn key_kind(self) -> SortKeyKind {
        match self {
            Self::Year => SortKeyKind::Int,
            _ => SortKeyKind::Text,
        }
    }

    fn order_sql(self, order: SortOrder) -> String {
        let dir = match order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        };
        format!("{} {dir}, a.id ASC", self.sort_sql())
    }

    fn primary_key(self, row: &AlbumListRow) -> SortKeyValue {
        match self {
            Self::Title => SortKeyValue::Text(row.title.clone()),
            Self::Artist => SortKeyValue::Text(row.artist_name.clone()),
            Self::Year => SortKeyValue::Int(row.year.unwrap_or(-1) as i64),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlbumsListParams {
    pub sort: AlbumsSort,
    pub order: SortOrder,
    pub limit: u32,
    pub q: Option<String>,
    pub cursor: Option<String>,
}

pub async fn list_keyset(
    pool: &SqlitePool,
    params: AlbumsListParams,
) -> Result<KeysetPage<AlbumListRow>, ApiError> {
    let fingerprint = fingerprint_json(&json!({ "q": params.q }));

    let mut keyset_clause = String::new();
    let mut keyset_binds: Vec<SortKeyValue> = Vec::new();
    if let Some(ref cursor_str) = params.cursor {
        let payload = decode_cursor(cursor_str)?;
        let (primary, tie) = ensure_cursor_matches(
            &payload,
            params.sort.as_str(),
            params.order,
            &fingerprint,
            params.sort.key_kind(),
        )?;
        let (clause, binds) =
            keyset_and_clause(params.order, params.sort.sort_sql(), "a.id", &primary, tie);
        keyset_clause = clause;
        keyset_binds = binds;
    }

    let mut search_clause = String::new();
    let mut search_binds: Vec<String> = Vec::new();
    if let Some(ref q) = params.q
        && !q.trim().is_empty()
    {
        search_clause = " AND (a.title LIKE ? OR COALESCE(ar.name, '') LIKE ?)".to_string();
        let pattern = format!("%{}%", q.trim());
        search_binds.push(pattern.clone());
        search_binds.push(pattern);
    }

    let fetch_limit = (params.limit as i64) + 1;
    let order_by = params.sort.order_sql(params.order);
    let sql = format!(
        r#"
        SELECT
            a.id,
            a.title,
            COALESCE(ar.name, '') AS artist_name,
            a.year,
            a.path,
            a.cover_path,
            (SELECT COUNT(*) FROM tracks t WHERE t.album_id = a.id) AS track_count
        FROM albums a
        LEFT JOIN artists ar ON a.artist_id = ar.id
        WHERE 1=1
        {search_clause}
        {keyset_clause}
        ORDER BY {order_by}
        LIMIT ?
        "#
    );

    let mut query = sqlx::query_as::<_, AlbumListRow>(&sql);
    for p in &search_binds {
        query = query.bind(p);
    }
    query = bind_sort_keys(query, &keyset_binds);
    query = query.bind(fetch_limit);

    let rows: Vec<AlbumListRow> = query.fetch_all(pool).await?;
    let sort = params.sort;
    Ok(finish_keyset_page(
        rows,
        params.limit as usize,
        sort.as_str(),
        params.order,
        &fingerprint,
        |r| (sort.primary_key(r), r.id),
    ))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AlbumListRow {
    pub id: i64,
    pub title: String,
    pub artist_name: String,
    pub year: Option<i32>,
    pub path: Option<String>,
    pub cover_path: Option<String>,
    pub track_count: i64,
}

pub async fn set_cover_path(pool: &SqlitePool, id: i64, cover_path: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    if !catalog::set_album_cover_path(&handle, id, cover_path).await? {
        return Err(ApiError::Message("album not found".into()));
    }
    Ok(())
}

pub async fn id_by_path(pool: &SqlitePool, path: &str) -> Result<Option<i64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::album_id_by_path(&handle, path).await?)
}

pub async fn delete_empty_storage_albums_in_scope(
    pool: &SqlitePool,
    scope_path: Option<&str>,
) -> Result<u64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::delete_empty_storage_albums_in_scope(&handle, scope_path).await?)
}

pub(crate) fn path_prefix_bounds(path: &str) -> (String, String) {
    let lower = format!("{}/", path.trim_end_matches('/'));
    let mut upper = lower.clone().into_bytes();
    if let Some(last) = upper.last_mut() {
        *last = last.saturating_add(1);
    }
    (
        lower,
        String::from_utf8(upper).expect("path prefix upper bound remains valid UTF-8"),
    )
}

pub async fn find_id_by_qobuz_album_id(
    pool: &SqlitePool,
    qobuz_id: i64,
) -> Result<Option<i64>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::find_album_id_by_qobuz_album_id(&handle, qobuz_id).await?)
}

fn album_row_from_data(row: catalog::AlbumRow) -> AlbumRow {
    AlbumRow {
        id: row.id,
        artist_id: row.artist_id,
        title: row.title,
        year: row.year,
        qobuz_album_id: row.qobuz_album_id,
        path: row.path,
        cover_path: row.cover_path,
    }
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

    #[tokio::test]
    async fn delete_empty_storage_albums_in_scope_keeps_non_empty_and_metadata_only() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let artist_id = artists::upsert_by_name(&pool, "A", None).await.unwrap();
        let empty_in_scope = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Empty",
                year: None,
                qobuz_album_id: None,
                path: Some("A/Empty"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let non_empty = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Full",
                year: None,
                qobuz_album_id: None,
                path: Some("A/Full"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let empty_outside_scope = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Outside",
                year: None,
                qobuz_album_id: None,
                path: Some("B/Outside"),
                cover_path: None,
            },
        )
        .await
        .unwrap();
        let metadata_only = upsert(
            &pool,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Metadata",
                year: None,
                qobuz_album_id: Some(42),
                path: None,
                cover_path: None,
            },
        )
        .await
        .unwrap();
        crate::db::tracks::upsert(
            &pool,
            crate::db::tracks::TrackUpsert {
                album_id: non_empty,
                title: "Track",
                track_number: None,
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path: "A/Full/01.flac",
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();

        let deleted = delete_empty_storage_albums_in_scope(&pool, Some("A"))
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        assert!(get_by_id(&pool, empty_in_scope).await.unwrap().is_none());
        assert!(get_by_id(&pool, non_empty).await.unwrap().is_some());
        assert!(
            get_by_id(&pool, empty_outside_scope)
                .await
                .unwrap()
                .is_some()
        );
        assert!(get_by_id(&pool, metadata_only).await.unwrap().is_some());
    }
}

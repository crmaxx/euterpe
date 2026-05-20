use serde_json::json;
use sqlx::SqlitePool;

use crate::api::keyset::{
    decode_cursor, ensure_cursor_matches, fingerprint_json, finish_keyset_page, keyset_and_clause,
};
use crate::api::{KeysetPage, SortKeyKind, SortKeyValue, SortOrder};
use crate::error::ApiError;

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
    let n =
        sqlx::query("UPDATE albums SET cover_path = ?, updated_at = datetime('now') WHERE id = ?")
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
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM albums WHERE qobuz_album_id = ?")
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

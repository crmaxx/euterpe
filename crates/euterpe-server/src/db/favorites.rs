use chrono::Utc;
use serde_json::json;
use sqlx::SqlitePool;

use crate::api::keyset::{
    decode_cursor, ensure_cursor_matches, finish_keyset_page, fingerprint_json, keyset_and_clause,
};
use crate::api::{KeysetPage, QobuzFavoriteItem, SortKeyKind, SortKeyValue, SortOrder};
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

#[derive(Debug, sqlx::FromRow)]
struct FavoriteRow {
    qobuz_id: i64,
    album_api_id: Option<String>,
    title: Option<String>,
    artist_name: Option<String>,
    cover_url: Option<String>,
    local_album_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoritesSort {
    Title,
    Artist,
    InLibrary,
}

impl FavoritesSort {
    pub fn parse(s: &str) -> Result<Self, ApiError> {
        match s {
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            "in_library" => Ok(Self::InLibrary),
            _ => Err(ApiError::bad_request(
                "sort must be title, artist, or in_library",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::InLibrary => "in_library",
        }
    }

    fn sort_sql(self) -> &'static str {
        match self {
            Self::Title => "COALESCE(f.title, '') COLLATE NOCASE",
            Self::Artist => "COALESCE(f.artist_name, '') COLLATE NOCASE",
            Self::InLibrary => "CASE WHEN a.id IS NOT NULL THEN 1 ELSE 0 END",
        }
    }

    fn key_kind(self) -> SortKeyKind {
        match self {
            Self::InLibrary => SortKeyKind::Bool,
            _ => SortKeyKind::Text,
        }
    }

    fn order_sql(self, order: SortOrder) -> String {
        let dir = match order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        };
        format!("{} {dir}, f.qobuz_id ASC", self.sort_sql())
    }

    fn primary_key(self, row: &FavoriteRow) -> SortKeyValue {
        match self {
            Self::Title => SortKeyValue::Text(row.title.clone().unwrap_or_default()),
            Self::Artist => SortKeyValue::Text(row.artist_name.clone().unwrap_or_default()),
            Self::InLibrary => SortKeyValue::Bool(if row.local_album_id.is_some() {
                1
            } else {
                0
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FavoritesListParams {
    pub sort: FavoritesSort,
    pub order: SortOrder,
    pub limit: u32,
    pub q: Option<String>,
    pub in_library: Option<bool>,
    pub cursor: Option<String>,
}

fn row_to_item(r: FavoriteRow) -> QobuzFavoriteItem {
    QobuzFavoriteItem {
        album_api_id: r
            .album_api_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| r.qobuz_id.to_string()),
        qobuz_id: r.qobuz_id,
        title: r.title.unwrap_or_default(),
        artist_name: r.artist_name.unwrap_or_default(),
        in_library: r.local_album_id.is_some(),
        local_album_id: r.local_album_id,
        cover_url: r.cover_url.filter(|s| !s.trim().is_empty()),
    }
}

/// `album_api_id` is stored in `slug` column: short ref, long slug, or catalog id for `album/get`.
pub async fn upsert_album(
    pool: &SqlitePool,
    qobuz_id: u64,
    title: &str,
    artist_name: &str,
    album_api_id: Option<&str>,
    cover_url: Option<&str>,
) -> Result<bool, ApiError> {
    let synced_at = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        INSERT INTO qobuz_favorites (entity_type, qobuz_id, title, artist_name, slug, cover_url, synced_at, removed)
        VALUES ('album', ?, ?, ?, ?, ?, ?, 0)
        ON CONFLICT(entity_type, qobuz_id) DO UPDATE SET
            title = excluded.title,
            artist_name = excluded.artist_name,
            slug = excluded.slug,
            cover_url = COALESCE(excluded.cover_url, qobuz_favorites.cover_url),
            synced_at = excluded.synced_at,
            removed = 0
        "#,
    )
    .bind(qobuz_id as i64)
    .bind(title)
    .bind(artist_name)
    .bind(album_api_id)
    .bind(cover_url)
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

pub async fn list_albums_keyset(
    pool: &SqlitePool,
    params: FavoritesListParams,
) -> Result<KeysetPage<QobuzFavoriteItem>, ApiError> {
    let fingerprint = fingerprint_json(&json!({
        "q": params.q,
        "in_library": params.in_library,
    }));

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
        let (clause, binds) = keyset_and_clause(
            params.order,
            params.sort.sort_sql(),
            "f.qobuz_id",
            &primary,
            tie,
        );
        keyset_clause = clause;
        keyset_binds = binds;
    }

    let mut filters = String::new();
    let mut filter_binds: Vec<String> = Vec::new();
    if let Some(in_lib) = params.in_library {
        if in_lib {
            filters.push_str(" AND a.id IS NOT NULL");
        } else {
            filters.push_str(" AND a.id IS NULL");
        }
    }
    if let Some(ref q) = params.q {
        if !q.trim().is_empty() {
            filters.push_str(" AND (f.title LIKE ? OR f.artist_name LIKE ?)");
            let pattern = format!("%{}%", q.trim());
            filter_binds.push(pattern.clone());
            filter_binds.push(pattern);
        }
    }

    let fetch_limit = (params.limit as i64) + 1;
    let order_by = params.sort.order_sql(params.order);
    let sql = format!(
        r#"
        SELECT
            f.qobuz_id,
            f.slug AS album_api_id,
            f.title,
            f.artist_name,
            f.cover_url,
            a.id AS local_album_id
        FROM qobuz_favorites f
        LEFT JOIN albums a ON a.qobuz_album_id = f.qobuz_id
        WHERE f.entity_type = 'album' AND f.removed = 0
        {filters}
        {keyset_clause}
        ORDER BY {order_by}
        LIMIT ?
        "#
    );

    let mut query = sqlx::query_as::<_, FavoriteRow>(&sql);
    for p in &filter_binds {
        query = query.bind(p);
    }
    query = bind_sort_keys(query, &keyset_binds);
    query = query.bind(fetch_limit);

    let rows: Vec<FavoriteRow> = query.fetch_all(pool).await?;
    let sort = params.sort;
    let page = finish_keyset_page(
        rows,
        params.limit as usize,
        sort.as_str(),
        params.order,
        &fingerprint,
        |r| (sort.primary_key(r), r.qobuz_id),
    );
    Ok(KeysetPage {
        items: page.items.into_iter().map(row_to_item).collect(),
        next_cursor: page.next_cursor,
        has_more: page.has_more,
    })
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

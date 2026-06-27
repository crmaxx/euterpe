use sqlx::SqlitePool;

use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::catalog;

pub async fn upsert_by_name(
    pool: &SqlitePool,
    name: &str,
    qobuz_artist_id: Option<i64>,
) -> Result<i64, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::upsert_artist_by_name(&handle, name, qobuz_artist_id).await?)
}

pub async fn name_by_id(pool: &SqlitePool, id: i64) -> Result<Option<String>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(catalog::get_artist_name_by_id(&handle, id).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::{connect, migrate};

    #[tokio::test]
    async fn upsert_artist_returns_stable_id() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let a = upsert_by_name(&pool, "Artist A", None).await.unwrap();
        let b = upsert_by_name(&pool, "Artist A", None).await.unwrap();
        assert_eq!(a, b);
    }
}

use sqlx::SqlitePool;

use crate::error::ApiError;
use euterpe_data::DataHandle;
use euterpe_data::repositories::settings as data_settings;

pub const KEY_QOBUZ_USER_ID: &str = data_settings::KEY_QOBUZ_USER_ID;
pub const KEY_QOBUZ_UAT_ENC: &str = data_settings::KEY_QOBUZ_UAT_ENC;
pub const KEY_QOBUZ_ACTIVE_ACCOUNT_ID: &str = data_settings::KEY_QOBUZ_ACTIVE_ACCOUNT_ID;

pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>, ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    Ok(data_settings::get(&handle, key).await?)
}

pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data_settings::set(&handle, key, value).await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, key: &str) -> Result<(), ApiError> {
    let handle = DataHandle::from_sqlite_pool(pool.clone());
    data_settings::delete(&handle, key).await?;
    Ok(())
}

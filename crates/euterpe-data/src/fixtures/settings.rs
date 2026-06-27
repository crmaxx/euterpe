use crate::connection::DataHandle;
use crate::error::Result;
use crate::repositories::settings;

pub async fn set(handle: &DataHandle, key: &str, value: &str) -> Result<()> {
    settings::set(handle, key, value).await
}

pub async fn get(handle: &DataHandle, key: &str) -> Result<Option<String>> {
    settings::get(handle, key).await
}

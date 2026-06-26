use crate::connection::DataHandle;
use crate::error::Result;
use welds::WeldsModel;

pub const KEY_QOBUZ_USER_ID: &str = "qobuz.user_id";
pub const KEY_QOBUZ_UAT_ENC: &str = "qobuz.uat_enc";
pub const KEY_QOBUZ_ACTIVE_ACCOUNT_ID: &str = "qobuz.active_account_id";

#[derive(Debug, WeldsModel)]
#[welds(table = "settings")]
struct Setting {
    #[welds(primary_key)]
    key: String,
    value: String,
    updated_at: String,
}

pub async fn get(handle: &DataHandle, key: &str) -> Result<Option<String>> {
    let row = Setting::find_by_id(handle.client(), key.to_string()).await?;
    Ok(row.map(|setting| setting.value.clone()))
}

pub async fn set(handle: &DataHandle, key: &str, value: &str) -> Result<()> {
    let now = sqlite_timestamp();
    if let Some(mut setting) = Setting::find_by_id(handle.client(), key.to_string()).await? {
        setting.value = value.to_string();
        setting.updated_at = now;
        setting.save(handle.client()).await?;
        return Ok(());
    }

    let mut setting = Setting::new();
    setting.key = key.to_string();
    setting.value = value.to_string();
    setting.updated_at = now;
    setting.save(handle.client()).await?;
    Ok(())
}

pub async fn delete(handle: &DataHandle, key: &str) -> Result<()> {
    if let Some(mut setting) = Setting::find_by_id(handle.client(), key.to_string()).await? {
        setting.delete(handle.client()).await?;
    }
    Ok(())
}

fn sqlite_timestamp() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

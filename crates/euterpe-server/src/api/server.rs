use serde::Serialize;

use crate::api::StorageLocationView;
use crate::services::app_settings::UiPreferences;

#[derive(Debug, Clone, Serialize)]
pub struct ServerInfoResponse {
    pub version: String,
    pub library_storage: Option<StorageLocationView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torrent_incoming_dir: Option<String>,
    pub credentials_configured: bool,
    pub admin_auth_required: bool,
    pub ui: UiPreferences,
}

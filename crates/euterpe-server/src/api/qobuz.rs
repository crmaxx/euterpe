use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzTestLoginRequest {
    pub user_id: u64,
    pub auth_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzTestLoginResponse {
    pub membership: String,
    pub user_auth_token_refreshed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzSyncResponse {
    pub run_id: i64,
    pub albums_total: i64,
    pub added: i64,
    pub removed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzSyncRunSummary {
    pub id: i64,
    pub status: String,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub albums_total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub albums_added: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub albums_removed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzSyncLatestResponse {
    pub run: Option<QobuzSyncRunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzFavoriteItem {
    /// Value for `album/get` and `POST /downloads` (`album_api_id`).
    pub album_api_id: String,
    /// Catalog id when known (from `qobuz_id` in Qobuz JSON).
    pub qobuz_id: i64,
    pub title: String,
    pub artist_name: String,
    pub in_library: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_album_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzFavoritesListResponse {
    pub items: Vec<QobuzFavoriteItem>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzFavoritesMutateRequest {
    pub album_ids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzOAuthStartResponse {
    pub authorize_url: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzConnectionStatusResponse {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_account_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qobuz_user_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub membership_label: Option<String>,
    pub master_key_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzAccountListItem {
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub qobuz_user_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub membership_label: Option<String>,
    pub uat_obtained_at: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzAccountsListResponse {
    pub items: Vec<QobuzAccountListItem>,
}

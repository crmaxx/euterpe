use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QobuzTestLoginRequest {
    pub user_id: u64,
    pub auth_token: String,
    #[serde(default)]
    pub persist: bool,
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

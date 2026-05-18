use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde::Deserialize;

use crate::api::{
    QobuzAccountListItem, QobuzAccountsListResponse, QobuzConnectionStatusResponse,
    QobuzOAuthStartResponse,
};
use crate::credentials;
use crate::db::{qobuz_accounts, settings};
use crate::db::settings::KEY_QOBUZ_ACTIVE_ACCOUNT_ID;
use crate::error::ApiError;
use crate::services::qobuz_oauth;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    /// Qobuz redirects with `code_autorisation` (French spelling); `code` is accepted too.
    #[serde(alias = "code_autorisation", alias = "code_authorization")]
    pub code: String,
    pub state: Option<String>,
}

pub async fn oauth_start(State(state): State<AppState>) -> Result<Json<QobuzOAuthStartResponse>, ApiError> {
    let start = qobuz_oauth::oauth_start(&state).await?;
    Ok(Json(QobuzOAuthStartResponse {
        authorize_url: start.authorize_url,
        state: start.state,
    }))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Query(q): Query<OAuthCallbackQuery>,
) -> Result<Response, ApiError> {
    let account_id = qobuz_oauth::oauth_callback(&state, &q.code, q.state.as_deref()).await?;
    let redirect_to = format!(
        "{}/settings?qobuz=connected&account_id={}",
        state.config.public_base_url, account_id
    );
    Ok(Redirect::temporary(&redirect_to).into_response())
}

pub async fn connection_status(
    State(state): State<AppState>,
) -> Result<Json<QobuzConnectionStatusResponse>, ApiError> {
    let connected = credentials::load_active(&state.config, &state.db)
        .await?
        .is_some();
    let active_account_id = settings::get(&state.db, KEY_QOBUZ_ACTIVE_ACCOUNT_ID)
        .await?
        .and_then(|s| s.parse().ok());

    let mut qobuz_user_id = None;
    let mut display_name = None;
    let mut membership_label = None;
    if let Some(id) = active_account_id {
        if let Some(row) = qobuz_accounts::get_by_id(&state.db, id).await? {
            qobuz_user_id = Some(row.qobuz_user_id);
            display_name = row.display_name;
            membership_label = row.membership_label;
        }
    }

    Ok(Json(QobuzConnectionStatusResponse {
        connected,
        active_account_id,
        qobuz_user_id,
        display_name,
        membership_label,
        master_key_configured: state.config.master_key.is_some(),
    }))
}

pub async fn logout(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    credentials::disconnect_active(&state.db).await?;
    state.reload_qobuz_from_db().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_accounts(
    State(state): State<AppState>,
) -> Result<Json<QobuzAccountsListResponse>, ApiError> {
    let rows = qobuz_accounts::list_without_uat(&state.db).await?;
    let active_account_id = settings::get(&state.db, KEY_QOBUZ_ACTIVE_ACCOUNT_ID)
        .await?
        .and_then(|s| s.parse().ok());
    let items = rows
        .into_iter()
        .map(|r| QobuzAccountListItem {
            id: r.id,
            label: r.label,
            qobuz_user_id: r.qobuz_user_id,
            display_name: r.display_name,
            membership_label: r.membership_label,
            uat_obtained_at: r.uat_obtained_at,
            is_active: active_account_id == Some(r.id),
        })
        .collect();
    Ok(Json(QobuzAccountsListResponse { items }))
}

use crate::credentials;
use crate::db::qobuz_accounts;
use crate::error::ApiError;
use crate::state::AppState;
use chrono::{Duration as ChronoDuration, Utc};
use euterpe_qobuz::{
    authorize_url, fetch_oauth_bootstrap, login_with_oauth_code, redirect_uri_with_state,
};
use reqwest::Client;

pub struct OAuthStart {
    pub authorize_url: String,
    pub state: String,
}

pub async fn oauth_start(state: &AppState) -> Result<OAuthStart, ApiError> {
    state.master_key()?;
    let http = Client::new();
    let bootstrap = fetch_oauth_bootstrap(state.config.qobuz_play_base(), &http)
        .await
        .map_err(ApiError::from)?;

    let oauth_state = new_oauth_state();
    let expires = Utc::now()
        + ChronoDuration::from_std(state.config.oauth_state_ttl)
            .map_err(|e| ApiError::Config(format!("invalid oauth state ttl: {e}")))?;

    qobuz_accounts::purge_expired_oauth_states(&state.db).await?;
    qobuz_accounts::insert_oauth_state(&state.db, &oauth_state, expires).await?;

    let redirect_uri = redirect_uri_with_state(&state.config.oauth_callback_url(), &oauth_state);
    let url = authorize_url(&bootstrap.app_id, &redirect_uri);

    Ok(OAuthStart {
        authorize_url: url,
        state: oauth_state,
    })
}

pub async fn oauth_callback(
    state: &AppState,
    code: &str,
    oauth_state: Option<&str>,
) -> Result<i64, ApiError> {
    let master = state.master_key()?;

    let state_ok = match oauth_state.filter(|s| !s.is_empty()) {
        Some(s) => qobuz_accounts::consume_oauth_state(&state.db, s).await?,
        None => qobuz_accounts::consume_sole_pending_oauth_state(&state.db)
            .await?
            .is_some(),
    };
    if !state_ok {
        return Err(ApiError::bad_request(
            "invalid or expired OAuth state (restart connect from Settings)",
        ));
    }

    let http = Client::new();
    let bootstrap = fetch_oauth_bootstrap(state.config.qobuz_play_base(), &http)
        .await
        .map_err(ApiError::from)?;

    let login = login_with_oauth_code(
        &http,
        state.config.qobuz_api_base(),
        &bootstrap.app_id,
        &bootstrap.private_key,
        code,
    )
    .await
    .map_err(ApiError::from)?;

    let account_id = credentials::persist_oauth_account(&state.db, master, &login).await?;
    state.reload_qobuz_from_db().await?;
    Ok(account_id)
}

fn new_oauth_state() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}{:x}", nanos, std::process::id())
}

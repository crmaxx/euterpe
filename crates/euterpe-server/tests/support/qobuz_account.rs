use euterpe_server::AppState;

pub async fn seed_active_qobuz_account(state: &AppState, user_id: u64, token: &str) {
    use euterpe_qobuz::OAuthLoginResult;
    use euterpe_server::credentials;

    let master = state.master_key().expect("master key in test state");
    let login = OAuthLoginResult {
        user_id,
        user_auth_token: token.to_string(),
        display_name: None,
        membership_label: None,
    };
    credentials::persist_oauth_account(&state.db, master, &login)
        .await
        .expect("seed account");
}

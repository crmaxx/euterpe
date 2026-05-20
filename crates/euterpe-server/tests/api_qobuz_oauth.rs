//! OAuth flow with mocked Qobuz HTTP (no real network).

#[path = "support/qobuz_account.rs"]
mod qobuz_account;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

const BUNDLE_FIXTURE: &str = include_str!("../../euterpe-qobuz/tests/fixtures/bundle_sample.js");

fn bundle_with_private_key() -> String {
    format!("{BUNDLE_FIXTURE}\nprivateKey: \"pktest12\"\n")
}

#[tokio::test]
async fn oauth_start_returns_authorize_url_and_persists_state() {
    let mut server = mockito::Server::new_async().await;
    let play_base = server.url();
    let _login = server
        .mock("GET", "/login")
        .with_status(200)
        .with_body(r#"<script src="/resources/1.0.0-a000/bundle.js"></script>"#)
        .create_async()
        .await;
    let _bundle = server
        .mock("GET", "/resources/1.0.0-a000/bundle.js")
        .with_status(200)
        .with_body(bundle_with_private_key())
        .create_async()
        .await;

    let mut state = app::test_support::test_state().await;
    {
        let mut cfg = (*state.config).clone();
        cfg.public_base_url = "http://127.0.0.1:8080".into();
        cfg.qobuz_play_base = Some(play_base);
        state.config = std::sync::Arc::new(cfg);
    }

    let app = app::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/oauth/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let authorize_url = json["authorize_url"].as_str().unwrap();
    assert!(authorize_url.contains("www.qobuz.com/signin/oauth"));
    assert!(authorize_url.contains("ext_app_id=123456789"));
    assert!(authorize_url.contains("redirect_url="));
    assert!(authorize_url.contains("state%3D"));
    assert!(json["state"].as_str().unwrap().len() >= 8);
}

#[tokio::test]
async fn oauth_callback_exchanges_code_and_stores_account() {
    let mut server = mockito::Server::new_async().await;
    let play_base = server.url();
    let api_base = format!("{}/api.json/0.2", server.url());

    let _login = server
        .mock("GET", "/login")
        .with_status(200)
        .with_body(r#"<script src="/resources/1.0.0-a000/bundle.js"></script>"#)
        .create_async()
        .await;
    let _bundle = server
        .mock("GET", "/resources/1.0.0-a000/bundle.js")
        .with_status(200)
        .with_body(bundle_with_private_key())
        .create_async()
        .await;
    let _oauth_cb = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"/api\.json/0\.2/oauth/callback".to_string()),
        )
        .with_status(200)
        .with_body(r#"{"token":"uat-from-callback"}"#)
        .create_async()
        .await;
    let _user_login = server
        .mock(
            "POST",
            mockito::Matcher::Regex(r"/api\.json/0\.2/user/login".to_string()),
        )
        .with_status(200)
        .with_body(
            r#"{
            "user": {
                "id": 4242,
                "display_name": "Test User",
                "credential": { "parameters": { "short_label": "Studio" } }
            },
            "user_auth_token": "uat-final"
        }"#,
        )
        .create_async()
        .await;

    let mut state = app::test_support::test_state().await;
    let oauth_state = "test-oauth-state-abc";
    {
        let mut cfg = (*state.config).clone();
        cfg.public_base_url = "http://127.0.0.1:8080".into();
        cfg.qobuz_play_base = Some(play_base);
        cfg.qobuz_api_base = Some(api_base);
        state.config = std::sync::Arc::new(cfg);
    }

    euterpe_server::db::qobuz_accounts::purge_expired_oauth_states(&state.db)
        .await
        .unwrap();
    euterpe_server::db::qobuz_accounts::insert_oauth_state(
        &state.db,
        oauth_state,
        chrono::Utc::now() + chrono::Duration::minutes(10),
    )
    .await
    .unwrap();

    let app = app::app(state.clone());
    let uri = format!("/api/v1/qobuz/oauth/callback?code=auth-code-xyz&state={oauth_state}");
    let resp = app
        .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    let loc = resp.headers().get("location").unwrap().to_str().unwrap();
    assert!(loc.contains("/settings?qobuz=connected"));

    let creds = euterpe_server::credentials::load_active(&state.config, &state.db)
        .await
        .unwrap();
    assert!(creds.is_some());
    let creds = creds.unwrap();
    assert_eq!(creds.user_id, 4242);
    assert_eq!(creds.auth_token, "uat-final");
}

#[tokio::test]
async fn oauth_callback_accepts_code_autorisation_without_state_when_single_pending() {
    let mut server = mockito::Server::new_async().await;
    let play_base = server.url();
    let api_base = format!("{}/api.json/0.2", server.url());

    let _login = server
        .mock("GET", "/login")
        .with_status(200)
        .with_body(r#"<script src="/resources/1.0.0-a000/bundle.js"></script>"#)
        .create_async()
        .await;
    let _bundle = server
        .mock("GET", "/resources/1.0.0-a000/bundle.js")
        .with_status(200)
        .with_body(bundle_with_private_key())
        .create_async()
        .await;
    let _oauth_cb = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"/api\.json/0\.2/oauth/callback".to_string()),
        )
        .with_status(200)
        .with_body(r#"{"token":"uat-from-callback"}"#)
        .create_async()
        .await;
    let _user_login = server
        .mock(
            "POST",
            mockito::Matcher::Regex(r"/api\.json/0\.2/user/login".to_string()),
        )
        .with_status(200)
        .with_body(
            r#"{
            "user": { "id": 99, "display_name": "Fr User" },
            "user_auth_token": "uat-fr"
        }"#,
        )
        .create_async()
        .await;

    let mut state = app::test_support::test_state().await;
    {
        let mut cfg = (*state.config).clone();
        cfg.public_base_url = "http://127.0.0.1:8080".into();
        cfg.qobuz_play_base = Some(play_base);
        cfg.qobuz_api_base = Some(api_base);
        state.config = std::sync::Arc::new(cfg);
    }

    euterpe_server::db::qobuz_accounts::purge_expired_oauth_states(&state.db)
        .await
        .unwrap();
    euterpe_server::db::qobuz_accounts::insert_oauth_state(
        &state.db,
        "only-pending",
        chrono::Utc::now() + chrono::Duration::minutes(10),
    )
    .await
    .unwrap();

    let app = app::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/oauth/callback?code_autorisation=auth-code-fr")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);

    let creds = euterpe_server::credentials::load_active(&state.config, &state.db)
        .await
        .unwrap()
        .expect("connected");
    assert_eq!(creds.user_id, 99);
}

#[tokio::test]
async fn logout_clears_active_account() {
    let state = app::test_support::test_state().await;
    qobuz_account::seed_active_qobuz_account(&state, 4242, "uat-final").await;

    let app = app::app(state.clone());
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/qobuz/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let creds = euterpe_server::credentials::load_active(&state.config, &state.db)
        .await
        .unwrap();
    assert!(creds.is_none());

    let conn = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/qobuz/connection")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(conn.status(), StatusCode::OK);
    let bytes = conn.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["connected"], false);
}

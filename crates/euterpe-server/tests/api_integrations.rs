use axum::body::Body;
use axum::http::{Request, StatusCode};
use euterpe_server::app;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

mod support;

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn integrations_catalog_lists_tag_sources() {
    let state = support::test_state().await;
    let app = app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/integrations/catalog?type=tag_source")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let items = body["items"].as_array().unwrap();
    assert!(items.len() >= 4);
    let providers: Vec<_> = items
        .iter()
        .map(|i| i["provider"].as_str().unwrap())
        .collect();
    assert!(providers.contains(&"musicbrainz"));
    assert!(providers.contains(&"discogs"));
}

#[tokio::test]
async fn create_list_delete_musicbrainz_integration() {
    let state = support::test_state().await;
    let app = app(state);

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/integrations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "provider": "musicbrainz",
                        "type": "tag_source",
                        "config": { "contact": "test@example.com" }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created = json_body(create).await;
    let id = created["item"]["id"].as_i64().unwrap();

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/integrations?type=tag_source")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = json_body(list).await;
    assert!(
        list_body["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"].as_i64() == Some(id))
    );

    let del = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/integrations/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);
}

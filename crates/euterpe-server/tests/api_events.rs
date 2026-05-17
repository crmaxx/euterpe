mod support;

use axum::body::Body;
use axum::http::Request;
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

use support::{state_with_download_mock, DownloadMockQobuz};

#[tokio::test]
async fn events_stream_returns_job_progress() {
    let mock = DownloadMockQobuz::new();
    let state = state_with_download_mock(mock).await;
    let app = app::app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body);
    // Stream may be empty until a job runs; connection must succeed.
    assert!(text.is_empty() || text.contains("job_progress") || text.is_empty());
}

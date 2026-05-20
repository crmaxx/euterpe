#[path = "support/download_mock.rs"]
mod download_mock;

use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use euterpe_server::api::JobProgressEvent;
use euterpe_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

use download_mock::{DownloadMockQobuz, state_with_download_mock};

/// SSE never ends; do not `collect()` the body — read frames until `job_progress` or timeout.
async fn sse_text_until(mut body: Body, deadline: Duration) -> String {
    let started = tokio::time::Instant::now();
    let mut out = String::new();
    while started.elapsed() < deadline {
        let remain = deadline.saturating_sub(started.elapsed());
        let frame = match tokio::time::timeout(remain, body.frame()).await {
            Ok(Some(Ok(f))) => f,
            Ok(Some(Err(e))) => panic!("body frame error: {e}"),
            Ok(None) => break,
            Err(_) => break,
        };
        if let Some(chunk) = frame.data_ref() {
            out.push_str(&String::from_utf8_lossy(chunk));
        }
        if out.contains("job_progress") {
            break;
        }
    }
    out
}

#[tokio::test]
async fn events_stream_returns_job_progress() {
    let mock = DownloadMockQobuz::new();
    let state = state_with_download_mock(mock).await;
    let events_tx = state.events.clone();
    let app = app::app(state);

    let read = tokio::spawn(async move {
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

        sse_text_until(response.into_body(), Duration::from_secs(3)).await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = events_tx.send(JobProgressEvent {
        id: 42,
        progress_pct: 12.5,
        download_speed_bps: 0,
        torrent_detail: None,
    });

    let text = read.await.expect("read task");
    assert!(
        text.contains("job_progress"),
        "expected job_progress SSE event, got: {text:?}"
    );
}

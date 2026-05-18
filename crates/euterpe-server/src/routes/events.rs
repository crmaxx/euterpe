use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::api::{JobProgressEvent, ScanProgressEvent};
use crate::state::AppState;

enum SsePayload {
    Job(JobProgressEvent),
    Scan(ScanProgressEvent),
}

pub async fn subscribe_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let job_rx = state.events.subscribe();
    let scan_rx = state.scan_events.subscribe();
    let job_stream = BroadcastStream::new(job_rx).filter_map(|msg| {
        msg.ok().map(SsePayload::Job)
    });
    let scan_stream = BroadcastStream::new(scan_rx).filter_map(|msg| {
        msg.ok().map(SsePayload::Scan)
    });
    let stream = job_stream.merge(scan_stream).map(|payload| {
        let (event_name, data) = match payload {
            SsePayload::Job(p) => ("job_progress", serde_json::to_string(&p).unwrap_or_default()),
            SsePayload::Scan(p) => ("scan_progress", serde_json::to_string(&p).unwrap_or_default()),
        };
        Ok(Event::default().event(event_name).data(data))
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

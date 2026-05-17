# Очередь загрузок

Phase 3. In-process Tokio queue (не Redis на старте).

## Состояния job

```
queued → running → completed
              ↘ failed
              ↘ cancelled
```

## Worker model

```rust
// Single consumer channel
let (tx, mut rx) = mpsc::channel::<JobId>(32);

tokio::spawn(async move {
    while let Some(id) = rx.recv().await {
        if let Err(e) = run_job(&pool, &qobuz, id).await {
            mark_failed(id, e).await;
        }
    }
});
```

Concurrency: `Semaphore(3)` для параллельных HTTP download к CDN.

## run_job (album)

1. `album/get` → list track ids
2. For each track: `getFileUrl` → reqwest GET stream → write temp
3. Rename to final path per template
4. Update `progress_pct`
5. On complete — emit SSE

## SSE

`GET /api/v1/events` — `text/event-stream`

```
event: job_progress
data: {"id":1,"progress_pct":45.5}
```

## TDD

- Unit: state transitions `queued` → `running` legal only
- Unit: progress calculation tracks done / total
- Integration: mock Qobuz + write to tempdir

## Idempotency

Повторный POST download для того же `qobuz_album_id` + quality:

- если job `completed` и files exist → 409 или return existing
- policy в ADR later

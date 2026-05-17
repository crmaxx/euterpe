# Axum server (`euterpe-server`)

Phase 2+. Спецификация для TDD.

## Ответственность

- REST API для React
- Static file serve `frontend/dist`
- SQLite pool (sqlx)
- Spawn download workers (Tokio)
- SSE `/api/v1/events`

## Стек

- `axum` 0.7+
- `tower`, `tower-http` (CORS, trace, compression)
- `sqlx` sqlite
- `euterpe-qobuz` as dependency

## Структура crate

```
src/
├── main.rs
├── lib.rs
├── app.rs             # Router + handlers
├── config.rs
├── state.rs           # AppState: db, qobuz Arc<Mutex<dyn QobuzApi>>
├── api/               # DTO = OpenAPI components/schemas
├── middleware.rs      # optional EUTERPE_ADMIN_PASSWORD
├── crypto.rs          # AES-256-GCM for UAT at rest
├── credentials.rs
├── openapi.rs         # embedded spec → /api/openapi.json
├── db/
├── library/
│   └── paths.rs       # track path template + sanitize
├── routes/
│   ├── downloads.rs
│   └── events.rs      # SSE job_progress
├── services/
│   ├── qobuz_sync.rs
│   └── download/
│       └── worker.rs
```

## AppState

```rust
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub qobuz: Arc<Mutex<dyn QobuzApi>>,
    pub config: Arc<AppConfig>,
    pub job_tx: mpsc::Sender<i64>,
    pub events: broadcast::Sender<ServerEvent>,
}
```

## Middleware

- `TraceLayer`
- `CorsLayer` — только origins из config
- Auth middleware Phase 1

## TDD

`axum::Router` tests with `tower::ServiceExt`:

```rust
#[tokio::test]
async fn health_returns_ok() {
    let app = app(test_state());
    let res = app.oneshot(Request::get("/health")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

Mock `QobuzApi` trait for sync routes.

## OpenAPI-first

См. [openapi-first.ru.md](openapi-first.ru.md), [ADR 0006](../adr/0006-openapi-first.md).

Contract tests: `openapi_contract.rs`, `api_qobuz.rs`, `api_downloads.rs`, `api_events.rs`.

## Endpoints

Канон: [`openapi/openapi.yaml`](../../openapi/openapi.yaml). Обзор: [api-client.ru.md](../03-frontend/api-client.ru.md).

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
├── config.rs          # Env + figment
├── state.rs           # AppState: db, qobuz client arc
├── routes/
│   ├── mod.rs
│   ├── health.rs
│   ├── qobuz.rs
│   ├── downloads.rs
│   └── library.rs     # Phase 5
├── services/
│   ├── qobuz_sync.rs
│   └── download.rs
└── sse.rs
```

## AppState

```rust
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub qobuz: Arc<RwLock<QobuzClient>>,
    pub config: Arc<AppConfig>,
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

## Endpoints

См. [api-client.ru.md](../03-frontend/api-client.ru.md) (DRAFT).

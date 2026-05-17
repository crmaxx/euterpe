# Тестирование `euterpe-qobuz`

## Принцип

**Строгий TDD** — см. [development-process.ru.md](../00-overview/development-process.ru.md).

Нет PR без теста на новое поведение.

## Уровни

### Unit tests

- `signing.rs` — golden MD5 strings (fixed `request_ts`)
- `bundle.rs` — regex extract from checked-in `tests/fixtures/login_page.html` + `bundle_sample.js` (фрагменты, не полный bundle)
- `models/*` — deserialize fixtures
- `pagination.rs` — merge pages

Расположение: `src/foo.rs` `#[cfg(test)]` или `tests/*.rs`.

### HTTP integration (mock)

Crate: **mockito**

```rust
#[tokio::test]
async fn login_unauthorized() {
    let mut server = mockito::Server::new_async().await;
    let _m = server.mock("GET", "/api.json/0.2/user/login")
        .with_status(401)
        .create_async().await;
    // ...
}
```

Base URL override в `QobuzConfig::api_base` → `server.url()`.

### Live integration

File: `tests/integration_live.rs`

```rust
#[tokio::test]
#[ignore = "requires EUTERPE_QOBUZ_EMAIL and EUTERPE_QOBUZ_PASSWORD"]
async fn favorites_albums_live() { ... }
```

Env (**preferred 2026**):

| Variable | Описание |
|----------|----------|
| `EUTERPE_QOBUZ_USER_ID` | Numeric user id from `localuser` |
| `EUTERPE_QOBUZ_AUTH_TOKEN` | `userAuthToken` from browser / OAuth |
| `EUTERPE_QOBUZ_APP_ID` | optional, skip bundle |
| `EUTERPE_QOBUZ_SECRETS` | optional JSON array |

Legacy (deprecated, may 401):

| Variable | Описание |
|----------|----------|
| `EUTERPE_QOBUZ_EMAIL` | |
| `EUTERPE_QOBUZ_PASSWORD` | Do not use for new setups |

Запуск:

```bash
export EUTERPE_QOBUZ_USER_ID=...
export EUTERPE_QOBUZ_AUTH_TOKEN=...
cargo test -p euterpe-qobuz -- --ignored
```

## Golden vectors

`tests/signing_golden.rs`:

```rust
#[test]
fn track_get_file_url_signature() {
    let sig = sign_track_file_url(6, 12345678, 1715900000.0, "test_secret");
    assert_eq!(sig, "expected_md5_hex");
}
```

Векторы сверить с Python qopy / Go qobuz-sync один раз, зафиксировать в git.

## CI (будущее)

```yaml
- run: cargo test --workspace
- run: cargo test -p euterpe-qobuz -- --ignored
  if: github.event_name == 'schedule' && secrets.QOBUZ_TEST
```

Default PR: без live secrets.

## Coverage

Не гнаться за 100%. Обязательно:

- все `QobuzError` variants хотя бы один раз
- signing, pagination, login, favorites list

## Debug

Feature `raw_json` + CLI bin `euterpe-qobuz-debug` (Phase 1 optional):

```
euterpe-qobuz-debug album 12345
```

Печатает pretty JSON — как `qobuz-sync debug`.

# План реализации `euterpe-qobuz` (TDD)

Каждый milestone: **тесты → код → рефакторинг**. Не переходить к M{n+1}, пока M{n} не green.

## M1 — Bootstrap + Session auth (token-first)

> **2026:** M1 не требует email/password. Login-тесты — для `TokenLogin` и deprecated password.

### Тесты (сначала)

| # | Тест | Тип |
|---|------|-----|
| M1.1 | `parse_app_id_from_bundle` на fixture `bundle_sample.js` | unit |
| M1.2 | `decode_secrets` совпадает с эталоном | unit golden |
| M1.3 | `sign_track_file_url` golden vector | unit |
| M1.4 | `connect` SessionToken sets headers without password login call | unit/mock |
| M1.5 | mock `user/login` token flow 200 → UAT in state | integration |
| M1.6 | mock `user/login` 401 → `Authentication` + hint UseToken | integration |
| M1.7 | mock free account on token login → `Ineligible` | integration |
| M1.8 | `verify_session` calls favorites mock 200 | integration |

### Реализация

- `bundle.rs`, `signing.rs`, `api/auth.rs`, `client.rs`, `error.rs`, `config.rs` (`AuthConfig::SessionToken`)

### DoD

- `cargo test -p euterpe-qobuz` green
- Live: `EUTERPE_QOBUZ_USER_ID` + `EUTERPE_QOBUZ_AUTH_TOKEN` (не password)

---

## M2 — Favorites list

### Тесты

| # | Тест |
|---|------|
| M2.1 | deserialize `favorites_albums_page0.json` |
| M2.2 | `fetch_all` 2 pages from mock |
| M2.3 | mock getUserFavorites 200 → Page items |
| M2.4 | live `favorites_all_albums` `#[ignore]` |

### Реализация

- `api/favorites.rs`, `pagination.rs`, `FavoritesSignMode` fallback

### DoD

- Live test passes locally with env credentials
- Docs `05-qobuz/favorites.md` sign mode зафиксирован

---

## M3 — Favorites create/delete

### Тесты

| # | Тест |
|---|------|
| M3.1 | mock create with `album_ids=1,2` |
| M3.2 | mock delete |
| M3.3 | live round-trip `#[ignore]` |

### Реализация

- `favorite_add_*`, `favorite_remove_*`

### DoD

- Round-trip live: add → list contains → delete → list not contains

---

## M4 — track/getFileUrl

### Тесты

| # | Тест |
|---|------|
| M4.1 | golden sig |
| M4.2 | mock response with `url` → `StreamUrl` |
| M4.3 | mock restrictions → `NonStreamable` |
| M4.4 | `Quality::format_id()` all variants |
| M4.5 | live get url `#[ignore]` |

### Реализация

- `api/streaming.rs`, secret probe in bootstrap

---

## M5 — Catalog

### Тесты

| # | Тест |
|---|------|
| M5.1 | album/get fixture |
| M5.2 | artist/get 2 pages |
| M5.3 | live album + artist albums `#[ignore]` |

### Реализация

- `api/catalog.rs`

---

## M6 — OAuth crate helpers (опционально, после M5)

Низкоуровневые типы для OAuth (если нужны в `euterpe-qobuz`). **Полный OAuth UI + БД** — [FP-1](../00-overview/future-plans.ru.md) в `euterpe-server`, не в crate.

- TDD: mock token exchange payload parse
- Не блокирует Phase 1 homelab (manual UAT достаточно)

## Workspace setup (перед M1)

1. Root `Cargo.toml` workspace
2. `crates/euterpe-qobuz` с пустыми тестами M1.1 (fail)
3. CI: `cargo test --workspace` (без ignored)

Порядок создания файлов — **только после** соответствующего failing test.

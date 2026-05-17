# Дорожная карта

Все фазы — **строгий TDD** ([development-process.ru.md](development-process.ru.md)).

## Phase 0 — Документация (текущая)

- Структура `docs/`
- Спецификация Qobuz API и crate `euterpe-qobuz`
- ADR, Docker compose example

**DoD:** навигация README.ru.md; нет противоречий между `05-qobuz` и `06-library`.

## Phase 1 — `euterpe-qobuz`

Milestones M1–M5 ([implementation-plan.ru.md](../06-library-euterpe-qobuz/implementation-plan.ru.md)):

- M1: bundle + login (tests first)
- M2: favorites list
- M3: favorites create/delete
- M4: track/getFileUrl
- M5: album/get, artist/get paginated

**DoD:** `cargo test -p euterpe-qobuz` green; live tests documented.

## Phase 2 — Backend core

- Cargo workspace, `euterpe-server`
- SQLite + sqlx migrations
- Qobuz credentials в settings (encrypted)
- `POST /api/v1/qobuz/sync`, `GET /api/v1/qobuz/favorites`
- TDD: axum tests per route

## Phase 3 — Download pipeline

- `download_jobs`, Tokio worker
- Stream URL → file on `/music`
- SSE progress
- TDD: job state machine unit tests

## Phase 4 — Frontend

- Vite + React + shadcn
- Screens: Settings, Favorites, Queue
- TDD: Vitest + MSW

## Phase 5 — Library & tags

- Filesystem rescan
- `lofty` tag read/write
- Cover embed
- TDD: tag round-trip fixtures

## Phase 6+ — Qobuz UX и multi-account (будущее)

См. детали: [future-plans.ru.md](future-plans.ru.md).

### FP-1 — Токен из приложения (OAuth → БД)

- OAuth flow в UI Euterpe (`/api/v1/qobuz/oauth/start|callback`)
- Сохранение `user_auth_token` в `qobuz_accounts` (encrypted)
- Без ручной вставки токена в env (env остаётся fallback)
- TDD: mock OAuth + DB round-trip

**После:** Phase 2 backend, вместе с Phase 4 Settings UI.

### FP-2 — Выбор пользователя Qobuz

- Несколько привязанных аккаунтов Qobuz на одном инстансе
- `qobuz.active_account_id` — от кого sync / favorites / downloads
- UI: переключатель аккаунта в header/settings
- `qobuz_favorites`, `download_jobs`, `qobuz_sync_runs` scoped by `qobuz_account_id`
- TDD: switch active → API uses correct mock client

**После:** FP-1 (желательно) или параллельно с Phase 4.

### Порядок внедрения (рекомендуемый)

```
Phase 2 (API + один аккаунт, env/token)
  → FP-1 OAuth + qobuz_accounts table
  → FP-2 multi-account + UI switcher
  → Phase 3 downloads (уже с qobuz_account_id)
```

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
- **OpenAPI 3.1** (`openapi/openapi.yaml`), Redocly CI, contract tests
- SQLite + sqlx migrations (`settings`, `qobuz_favorites`, `qobuz_sync_runs`)
- Qobuz credentials: env или settings (AES-256-GCM + `EUTERPE_MASTER_KEY`)
- `POST /api/v1/qobuz/sync`, `GET/POST/DELETE /api/v1/qobuz/favorites`, `POST .../test-login`
- `GET /api/openapi.json`, `GET /health`
- TDD: axum + mock `QobuzApi` per route

## Phase 3 — Download pipeline ✅

- `download_jobs` (migration `002_phase3_download_jobs.sql`), Tokio worker + `mpsc` queue
- Album download: stream URL → `{EUTERPE_LIBRARY_PATH}/{artist}/{album}/…`
- REST: `POST/GET/DELETE /api/v1/downloads`, SSE `GET /api/v1/events`
- TDD: state machine unit tests, `api_downloads`, `api_events`

## Phase 4 — Frontend ✅

- Vite + React + shadcn (`frontend/`)
- Screens: Settings, Favorites, Queue, Library (placeholder)
- `GET /api/v1/server/info`, `GET /api/v1/qobuz/sync/latest`
- SPA static via `EUTERPE_STATIC_DIR` + Docker multi-stage
- TDD: Vitest + MSW (`npm run test` in `frontend/`)

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

### FP-3 — Очередь загрузок: очистка и удаление

- **Полная очистка** — удалить все старые jobs (`completed` / `failed` / `cancelled`), не трогая `queued` и `running`
- **Удаление по одному** — убрать конкретную запись из очереди (отдельно от cancel активного job)
- API + UI `/queue`; TDD: `api_downloads`, Vitest

### FP-4 — Favorites: сортировка

- Сортировка таблицы по **Title**, **Artist**, **In library** (клик по заголовку, asc/desc)
- Сначала client-side (TanStack Table); при необходимости — `sort`/`order` в API

**Целевая фаза:** Phase 4b. Детали: [future-plans.ru.md](future-plans.ru.md#fp-3--очередь-загрузок-очистка-и-удаление-заданий).

### Порядок внедрения (рекомендуемый)

```
Phase 2 (API + один аккаунт, env/token)
  → FP-1 OAuth + qobuz_accounts table
  → FP-2 multi-account + UI switcher
  → Phase 3 downloads (уже с qobuz_account_id)
```

# Дорожная карта

Все фазы — **строгий TDD** ([development-process.ru.md](development-process.ru.md)).

**Фазы 0–5 выполнены** (baseline продукта). Дальнейшие идеи — **Phase 6+** и пункты **FP-1…FP-10** в [future-plans.ru.md](future-plans.ru.md).

## Phase 0 — Документация ✅

- Структура `docs/`
- Спецификация Qobuz API и crate `euterpe-qobuz`
- ADR, Docker compose example

**DoD:** навигация README.ru.md; нет противоречий между `05-qobuz` и `06-library`.

## Phase 1 — `euterpe-qobuz` ✅

Milestones M1–M5 ([implementation-plan.ru.md](../06-library-euterpe-qobuz/implementation-plan.ru.md)):

- M1: bundle + login (tests first)
- M2: favorites list
- M3: favorites create/delete
- M4: track/getFileUrl
- M5: album/get, artist/get paginated

**DoD:** `cargo test -p euterpe-qobuz` green; live tests documented.

## Phase 2 — Backend core ✅

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
- Screens: Settings, Favorites, Queue, Library
- `GET /api/v1/server/info`, `GET /api/v1/qobuz/sync/latest`
- SPA static via `EUTERPE_STATIC_DIR` + Docker multi-stage
- TDD: Vitest + MSW (`npm run test` in `frontend/`)

## Phase 5 — Library & tags ✅

- Filesystem rescan (background job + SSE `scan_progress`)
- `lofty` tag read/write (`PATCH /api/v1/library/tracks/{id}`)
- Обложка **после скачивания с Qobuz:** файл **`cover.<ext>`** в каталоге альбома (расширение по MIME ответа) + embed во все треки (`covers.rs`, вызывается из download worker)
- UI `/library`: rescan, список альбомов/треков, превью обложки (`GET /api/v1/library/albums/{id}/cover`, плейсхолдер **No cover**), редактор **текстовых** тегов трека
- **Не в scope Phase 5:** загрузка/замена обложки **из приложения** — см. **FP-6** в [future-plans.ru.md](future-plans.ru.md#fp-6--обложка-альбома-загрузка-и-замена-из-ui); вручную можно положить **`cover.<ext>`** на диск
- TDD: tag round-trip fixtures, `api_library`, Vitest

## Phase 6+ — Qobuz UX и future plans (будущее)

См. детали: [future-plans.ru.md](future-plans.ru.md).

### FP-1 — Токен из приложения (OAuth → БД) ✅

- OAuth flow в UI Euterpe (`/api/v1/qobuz/oauth/start|callback`)
- Сохранение `user_auth_token` в `qobuz_accounts` (encrypted)
- Без ручной вставки токена в env

### FP-2 — Очередь: purge и удаление jobs ✅

- `POST /api/v1/downloads/purge`, `DELETE …?purge=1`
- UI: «Clear history», удаление строки
- См. [future-plans.ru.md — FP-2](future-plans.ru.md#fp-2--очередь-загрузок-очистка-и-удаление-заданий-)

### FP-3 — Favorites: сортировка и фильтр ✅

- **Сортировка на сервере:** `sort` / `order` в `GET …/favorites` + SQL `ORDER BY` — FP-3a–FP-3c
- Фильтр **в библиотеке / нет** — FP-3d; **поиск** — FP-3e; **обложки** — FP-3f
- См. [future-plans.ru.md — FP-3](future-plans.ru.md#fp-3--favorites-сортировка-таблицы)

### FP-4 — Индекс сразу после download

- После альбома: upsert `albums` + **все `tracks`** из `album/get` (без обязательного rescan)
- Сейчас: только `albums` (FP-7a); треки — FP-7b
- См. [future-plans.ru.md — FP-7](future-plans.ru.md#fp-7--библиотека-сразу-после-скачивания-без-обязательного-rescan)

### FP-5 — Автозаполнение тегов (каталоги)

- MusicBrainz, Discogs, GnuDB, TrackType.org — [future-plans.ru.md — FP-4](future-plans.ru.md#fp-4--автозаполнение-тегов-из-внешних-каталогов)

### FP-6 — Теги из Qobuz при скачивании

- `write_tags` после download — [future-plans.ru.md — FP-5](future-plans.ru.md#fp-5--автопроставление-тегов-из-qobuz-при-скачивании)

### FP-7 — Обложка альбома из UI

- Upload/replace cover — [future-plans.ru.md — FP-6](future-plans.ru.md#fp-6--обложка-альбома-загрузка-и-замена-из-ui)

### FP-8 — List API: keyset-пагинация ✅

- Единый контракт `limit` / `sort` / `order` / `cursor` — [future-plans.ru.md — FP-8](future-plans.ru.md#fp-8--коллекции-в-api-keyset-пагинация-и-сортировка)

### FP-9 — Параллельный library scan

- Очередь + пул воркеров (legacy/repair) — [future-plans.ru.md — FP-9](future-plans.ru.md#fp-9--параллельное-сканирование-library-очередь--пул-воркеров)

### FP-10 — Multi-account Qobuz ⏸

- Несколько аккаунтов, active switcher — отложено
- См. [future-plans.ru.md — FP-10](future-plans.ru.md#fp-10--выбор-активного-пользователя-qobuz--отложено)

### Порядок внедрения (рекомендуемый)

```
Phase 0–5 ✅ (текущий baseline)
  → FP-1 OAuth + qobuz_accounts table ✅
  → FP-2 queue purge ✅
  → FP-3…FP-9 по приоритету
  → FP-10 multi-account (когда понадобится)
```

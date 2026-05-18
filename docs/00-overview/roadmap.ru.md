# Дорожная карта

Все фазы — **строгий TDD** ([development-process.ru.md](development-process.ru.md)).

**Фазы 0–5 выполнены** (baseline продукта). Дальнейшие идеи — **Phase 6+** и пункты **FP-1…FP-7** в [future-plans.ru.md](future-plans.ru.md).

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
- **Не в scope Phase 5:** загрузка/замена обложки **из приложения** — см. **FP-7** в [future-plans.ru.md](future-plans.ru.md#fp-7-album-cover-ui); вручную можно положить **`cover.<ext>`** на диск
- TDD: tag round-trip fixtures, `api_library`, Vitest

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

### FP-4 — Favorites: сортировка и фильтр

- **Сортировка на сервере:** `sort` / `order` в `GET …/favorites` + SQL `ORDER BY` (клиент — manual sort / повторный запрос) — FP-4a–FP-4c в [future-plans.ru.md](future-plans.ru.md#fp-4--favorites-сортировка-таблицы)
- Фильтр по **в библиотеке / нет** — FP-4d
- **Поиск** — FP-4e (`q` на API при пагинации)
- **Обложки** в списке — FP-4f

**Целевая фаза:** Phase 2b (API) / Phase 4b (UI). Детали: [future-plans.ru.md](future-plans.ru.md#fp-4--favorites-сортировка-таблицы).

### FP-8 — После download: запись в `albums` и scan

- Worker upsert в `albums` с `qobuz_album_id` из job → корректное **«В библиотеке»** в избранном; опционально инкрементальный scan треков
- Детали: [future-plans.ru.md](future-plans.ru.md#fp-8-library-after-download)

### FP-5 — Автозаполнение тегов (каталоги)

- Запросы к **MusicBrainz**, **Discogs**, **GnuDB**, **TrackType.org** (приоритет, rate limits, ключи на сервере)
- UI: lookup → превью → запись в файл (`lofty`)
- Детали: [future-plans.ru.md](future-plans.ru.md#fp-5-metadata-lookup)

### FP-6 — Теги из Qobuz при скачивании

- После записи файла в worker вызывать **`write_tags`** из данных `album/get` + `TrackSummary` (title, album, artist, track #, год из даты релиза, Qobuz id в комментарии)
- «Максимум» полей — расширить модели **`euterpe-qobuz`** под реальный JSON API (жанр, диск, лейбл, ISRC и т.д.) и замапить в lofty; учесть `spawn_blocking`, порядок с embed обложки, skip при совпадении размера файла
- Детали: [future-plans.ru.md](future-plans.ru.md#fp-6-qobuz-download-tags)

### FP-7 — Обложка альбома: загрузка и замена из UI

- `PUT` / `POST multipart` на сервер (например `PUT /api/v1/library/albums/{id}/cover`) → валидация изображения → запись **`cover.<ext>`** (MIME → расширение, как при Qobuz-download) → обновление `albums.cover_path` → **re-embed** во все треки через `embed_cover_in_track` / `covers.rs`
- UI Library: кнопка «Заменить обложку», превью после успеха; опционально удаление обложки (очистка файла + `cover_path` + удаление picture из тегов)
- TDD: `api_library`, Vitest; проверка path traversal и лимита размера файла

**Целевая фаза:** **Phase 5b** / **Phase 6**. Детали: [future-plans.ru.md](future-plans.ru.md#fp-7-album-cover-ui)

### FP-9 — List API: keyset-пагинация и сортировка

- Единый контракт **`limit` / `sort` / `order` / `cursor`** (whitelist сортировки на ресурс; **без** `OFFSET`); ответ с **`next_cursor`** / **`has_more`**; UI — manual sort + запросы «следующая страница» по курсору (TanStack + react-query)
- Детали: [future-plans.ru.md](future-plans.ru.md#fp-9-api-collections-pagination-sort)

### Порядок внедрения (рекомендуемый)

```
Phase 0–5 ✅ (текущий baseline)
  → FP-1 OAuth + qobuz_accounts table
  → FP-2 multi-account + UI switcher
  → FP-3…FP-9 и др. по приоритету (см. future-plans.ru.md)
```

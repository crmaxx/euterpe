# REST API contract

**Канонический контракт:** [`openapi/openapi.yaml`](../../openapi/openapi.yaml) (OpenAPI 3.1). Этот файл — человекочитаемый обзор; при расхождении побеждает YAML.

Политика: [openapi-first.ru.md](../02-backend/openapi-first.ru.md). Изменения REST = сначала spec, затем contract tests + handlers.

Runtime: `GET /api/openapi.json` — spec в JSON.

Base path: `/api/v1`

## Server

### GET /api/v1/server/info

Public snapshot (no secrets): `version`, `library_path`, `credentials_configured`, `admin_auth_required`.

### GET /api/v1/qobuz/sync/latest

Latest row from `qobuz_sync_runs` or `{ "run": null }`.

## Health

### GET /health

```json
{ "status": "ok", "version": "0.1.0" }
```

## Qobuz

### POST /api/v1/qobuz/test-login

Проверка **user_auth_token** (и optional refresh via `user/login` token flow). Password не используется.

**Body:**

```json
{
  "user_id": 12345678,
  "auth_token": "..."
}
```

**Response 200:** `{ "membership": "Studio", "user_auth_token_refreshed": false }`  
**401:** invalid or expired token — UI показывает инструкцию обновления из play.qobuz.com

### Qobuz accounts (future FP-1 / FP-10)

| Method | Path | Описание |
|--------|------|----------|
| GET | `/api/v1/qobuz/accounts` | Список аккаунтов (без UAT) |
| POST | `/api/v1/qobuz/accounts` | Добавить (paste token или после OAuth) |
| POST | `/api/v1/qobuz/accounts/active` | `{ "account_id" }` — выбрать активного |
| GET | `/api/v1/qobuz/oauth/start` | **FP-1** — начать OAuth |
| GET | `/api/v1/qobuz/oauth/callback` | **FP-1** — сохранить токен в БД |

См. [future-plans.ru.md](../00-overview/future-plans.ru.md).

### POST /api/v1/qobuz/sync

Trigger favorites sync (albums default).

**Response 200:**

```json
{
  "run_id": 1,
  "albums_total": 120,
  "added": 3,
  "removed": 1
}
```

### GET /api/v1/qobuz/favorites?type=album&page=0&limit=50

```json
{
  "items": [
    {
      "qobuz_id": 123,
      "title": "Album",
      "artist_name": "Artist",
      "in_library": true,
      "local_album_id": 5
    }
  ],
  "total": 120
}
```

### POST /api/v1/qobuz/favorites

```json
{ "album_ids": [123, 456] }
```

### DELETE /api/v1/qobuz/favorites

```json
{ "album_ids": [123] }
```

## Downloads (Phase 3)

### POST /api/v1/downloads

```json
{
  "job_type": "album",
  "qobuz_id": 123,
  "quality": 6
}
```

**Response 202:** `{ "job_id": 42 }`

### GET /api/v1/downloads

List jobs.

### GET /api/v1/downloads/{id}

Job detail. **404** if missing.

### DELETE /api/v1/downloads/{id}

Cancel if queued/running. **204** on success; **409** if already completed.

Query `?status=queued|running|completed|failed|cancelled` on list.

## Events (Phase 3)

### GET /api/v1/events

SSE stream (`text/event-stream`). Events: `job_progress` with `{ "id", "progress_pct" }`.

## Errors

```json
{
  "error": {
    "code": "QOBUZ_AUTH_FAILED",
    "message": "Human readable"
  }
}
```

## TDD (server)

Один файл `crates/euterpe-server/tests/api_qobuz.rs` на endpoint.

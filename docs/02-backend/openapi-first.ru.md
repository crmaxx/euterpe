# OpenAPI-first

## Источник истины

Канонический контракт REST API: [`openapi/openapi.yaml`](../../openapi/openapi.yaml) (OpenAPI **3.1**).

Человекочитаемый обзор: [api-client.ru.md](../03-frontend/api-client.ru.md). При расхождении побеждает YAML.

## Цикл разработки

Вместе с [TDD](../00-overview/development-process.ru.md):

1. **RED** — изменить `openapi.yaml` (paths, schemas, `operationId`, examples).
2. **RED** — contract-тест: ответ handler валиден по JSON Schema из spec.
3. **GREEN** — handler + DTO в `crates/euterpe-server/src/api/` (имена 1:1 со схемами).
4. **REFACTOR** — только при зелёных lint + тестах.

Запрещено менять JSON ответа без diff в `openapi/openapi.yaml`.

## Документация (без сервера)

В `openapi/` — отдельный npm-проект (Redocly):

```bash
cd openapi && npm ci
npm run preview    # dev-сервер Redoc
npm run build      # статический HTML в openapi/dist/
```

См. [openapi/README.md](../../openapi/README.md).

## CI

```bash
cd openapi && npm ci && npm run lint && npm run build
```

Шаг выполняется в GitHub Actions на каждый push/PR.

## Runtime

| Endpoint | Назначение |
|----------|------------|
| `GET /api/openapi.json` | Spec в JSON (для Swagger/Redoc, codegen) |
| `GET /health` | Liveness |

## Phase 3 (реализовано)

`/api/v1/downloads` (CRUD) и `/api/v1/events` (SSE) описаны в `openapi/openapi.yaml` и реализованы в `euterpe-server`. Contract tests: `api_downloads.rs`, `api_events.rs`.

## Contract tests

`crates/euterpe-server/tests/openapi_contract.rs` — валидация тел ответов через `jsonschema` и схемы из `components/schemas`.

## Документация

См. [ADR 0006](../adr/0006-openapi-first.md).

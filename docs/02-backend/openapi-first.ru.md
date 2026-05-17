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

## CI

```bash
npx --yes @redocly/cli lint openapi/openapi.yaml
```

Шаг выполняется в GitHub Actions на каждый push/PR.

## Runtime

| Endpoint | Назначение |
|----------|------------|
| `GET /api/openapi.json` | Spec в JSON (для Swagger/Redoc, codegen) |
| `GET /health` | Liveness |

## Phase 3+ в spec

Endpoints вроде `/api/v1/downloads` могут присутствовать в YAML с `x-euterpe-phase: 3` до реализации. Server возвращает `501` до соответствующей фазы.

## Contract tests

`crates/euterpe-server/tests/openapi_contract.rs` — валидация тел ответов через `jsonschema` и схемы из `components/schemas`.

## Документация

См. [ADR 0006](../adr/0006-openapi-first.md).

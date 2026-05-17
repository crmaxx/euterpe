# ADR 0006: OpenAPI-first REST API

## Status

Accepted

## Context

Euterpe exposes a REST API to a future React UI and external automation. The API grows across phases (Qobuz sync, downloads, library). Markdown-only contracts drift from implementation. Code-first generators (e.g. `utoipa`) invert the desired review flow for a homelab product with a small, explicit surface.

## Decision

1. **Single source of truth:** `openapi/openapi.yaml` (OpenAPI 3.1).
2. **OpenAPI-first workflow:** change the spec before handlers; add contract tests; implement Rust DTOs matching `components/schemas`.
3. **CI gate:** Redocly lint on every PR.
4. **Runtime:** `GET /api/openapi.json` serves the spec as JSON.
5. Human-readable overview remains in `docs/03-frontend/api-client.ru.md` with a link to the YAML.

## Alternatives considered

| Approach | Rejected because |
|----------|------------------|
| Markdown only (`api-client.ru.md`) | No machine validation; easy drift |
| Code-first (`utoipa`) | Spec follows code; harder API review |
| Protobuf/gRPC | Overkill for browser + homelab |

## Consequences

### Positive

- Frontend can generate types (`openapi-typescript`) in Phase 4.
- Contract tests catch response shape regressions.
- PR reviewers see API diff in YAML.

### Negative

- Manual DTO maintenance until optional codegen ADR.
- Redocly requires Node in CI (acceptable).

## References

- [openapi-first.ru.md](../02-backend/openapi-first.ru.md)
- [ADR 0004](0004-test-driven-development.md)

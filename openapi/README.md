# OpenAPI contract

Canonical API specification: [`openapi.yaml`](openapi.yaml).

## Lint locally

```bash
npx --yes @redocly/cli lint openapi/openapi.yaml
```

## Bundle (optional)

```bash
npx --yes @redocly/cli bundle openapi/openapi.yaml -o openapi/bundled.yaml
```

## Runtime

`euterpe-server` serves the spec at `GET /api/openapi.json`.

See [docs/02-backend/openapi-first.ru.md](../docs/02-backend/openapi-first.ru.md).

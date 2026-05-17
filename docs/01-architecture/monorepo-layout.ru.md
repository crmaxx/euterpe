# Структура монорепозитория

Целевая структура:

```
euterpe/
├── Cargo.toml                 # workspace
├── README.md
├── openapi/
│   ├── openapi.yaml           # REST contract (OpenAPI 3.1)
│   ├── package.json           # Redocly: lint, preview, build HTML
│   ├── redocly.yaml
│   └── README.md
├── docs/                      # эта документация
├── crates/
│   ├── euterpe-qobuz/         # Phase 1 — TDD
│   └── euterpe-server/        # Phase 2 — Axum + SQLite
├── migrations/                # sqlx, Phase 2+
├── frontend/                  # Phase 4
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
├── docker/
│   ├── Dockerfile
│   └── nginx-spa.conf         # optional
└── .github/workflows/ci.yml   # cargo test + openapi lint/build
```

## Workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = ["crates/euterpe-qobuz", "crates/euterpe-server"]

[workspace.package]
edition = "2021"
license = "MIT OR Apache-2.0"
```

## Зависимости между crate

```
euterpe-server --> euterpe-qobuz
frontend       --> euterpe-server (HTTP only, no Rust dep)
```

## Frontend build output

`frontend/dist/` копируется в Docker image → `euterpe-server` serves static at `/`.

## TDD layout

| Path | Tests |
|------|-------|
| `crates/euterpe-qobuz/tests/` | library |
| `crates/euterpe-server/tests/` | axum + sqlx |
| `frontend/src/**/*.test.tsx` | vitest |

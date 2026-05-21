# Euterpe (Ἐυτέρπη)

[![CI](https://github.com/crmaxx/euterpe/actions/workflows/ci.yml/badge.svg)](https://github.com/crmaxx/euterpe/actions/workflows/ci.yml)

Self-hosted web application for managing a local music library with **Qobuz** synchronization.

Named after the muse of music and lyric poetry, daughter of Mnemosyne.

## Documentation

- **Russian (full):** [docs/README.ru.md](docs/README.ru.md)
- **English (index):** [docs/README.md](docs/README.md)

## Development

All implementation follows **strict Test-Driven Development (TDD)** — see [ADR 0004](docs/adr/0004-test-driven-development.md).

### One-time setup

```bash
make prepare   # overmind (macOS), cross, rustup targets, npm ci (husky + frontend), pre-commit hook
cp .env.example .env   # optional; loaded automatically at server start (cwd)
```

On commit, if `frontend/` or `openapi/` changed, the hook runs `generate:api` and `eslint` (same as CI).

### Tests

```bash
make test              # backend + frontend
make test-backend      # cargo test --workspace
make test-frontend     # npm test (runs generate:api first)
```

### Backend + frontend (recommended)

Uses [Overmind](https://github.com/DarthSim/overmind) and the root [`Procfile`](Procfile):

```bash
make dev
# API:  http://127.0.0.1:8080
# UI:   http://127.0.0.1:5173  (Vite proxies /api → backend)

make dev-stop          # or Ctrl+C in the overmind terminal
overmind connect backend
overmind connect frontend
```

### Backend only

```bash
make backend
# or: cargo run -p euterpe-server
```

### Frontend only

```bash
make frontend
# install → generate:api → dev (http://127.0.0.1:5173)
```

With a production build, the server serves the SPA from `frontend/dist` (or `EUTERPE_STATIC_DIR`):

```bash
cd frontend && npm run build
EUTERPE_STATIC_DIR=frontend/dist cargo run -p euterpe-server
# http://127.0.0.1:8080
```

## Stack

| Layer | Technology |
|-------|------------|
| Qobuz client | Rust crate `euterpe-qobuz` (reqwest, rustls) |
| API server | Axum, SQLite (WAL), sqlx |
| UI | Vite, React, Tailwind, TanStack Query/Table, shadcn/ui |
| Deploy | Docker (`/data` + `/music` volumes) |

## Qobuz authentication (2026)

Link Qobuz **in the web UI** (Settings → Connect Qobuz). The server stores an encrypted UAT in SQLite (`qobuz_accounts`); **`EUTERPE_MASTER_KEY`** is required.

See [docs/05-qobuz/oauth-and-tokens.ru.md](docs/05-qobuz/oauth-and-tokens.ru.md).

## Cross-compilation

Release binaries for Linux amd64, Windows amd64, and Raspberry Pi 1 (ARM1176JZF-S / DietPi):

```bash
make prepare          # installs cross if missing (see Makefile)
make cross-release-all
make dist-cross
```

See [docs/04-deployment/cross-compile.ru.md](docs/04-deployment/cross-compile.ru.md).

## Docker (preview)

Example compose (no secrets):

```bash
cp docs/04-deployment/compose.example.yml compose.yml
# set EUTERPE_MASTER_KEY, EUTERPE_PUBLIC_BASE_URL; link Qobuz via UI after start
docker compose up -d
```

See [docs/04-deployment/docker.ru.md](docs/04-deployment/docker.ru.md).

## Reference projects

Qobuz integration is informed by community tools (unofficial API):

- [qobuz-dl](https://github.com/vitiko98/qobuz-dl)
- [streamrip](https://github.com/nathom/streamrip)
- [qobuz-sync](https://github.com/trevorstarick/qobuz-sync)

## TODO

- Connect to network share
- Rework "Sources" page
- CUE split
- (Incremental?) Backups

## Disclaimer

Requires an active **Qobuz subscription**. This project is not affiliated with Qobuz. Use in compliance with [Qobuz API Terms of Use](https://static.qobuz.com/apps/api/QobuzAPI-TermsofUse.pdf). For personal / educational use.

## License

Licensed under the [Apache License, Version 2.0](LICENSE) (`Apache-2.0`). See [NOTICE](NOTICE) for attribution.

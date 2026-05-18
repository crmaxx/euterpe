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
make prepare   # overmind (macOS), npm ci (husky + frontend), git pre-commit hook
cp .env.example .env   # optional: Qobuz credentials and paths
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

## Future plans (short)

Delivered work is Phases **0–5** ([roadmap](docs/00-overview/roadmap.ru.md)). Next ideas ([`future-plans.ru.md`](docs/00-overview/future-plans.ru.md)):

1. **FP-1** — Qobuz OAuth in-app → encrypted token in DB (env stays fallback)
2. **FP-2** — Multiple Qobuz accounts + active switcher for sync/favorites/downloads
3. **FP-3** — Download queue: purge finished jobs + delete single history rows
4. **FP-4** — Favorites table sort (title / artist / in library)
5. **FP-5** — Metadata lookup from external catalogs (MusicBrainz, Discogs, …) → apply via `lofty`
6. **FP-6** — Auto-write tags from Qobuz `album/get` right after each track download
7. **FP-7** — Replace/upload album art from Library UI → `cover.<ext>` (MIME) + DB + re-embed in tracks

Details: [docs/00-overview/future-plans.ru.md](docs/00-overview/future-plans.ru.md) (Russian; full context per FP).

## Qobuz authentication (2026)

Automated **email/password API login is deprecated** (Qobuz uses OAuth on the website). Set **`EUTERPE_QOBUZ_USER_ID`** and **`EUTERPE_QOBUZ_AUTH_TOKEN`** from [play.qobuz.com](https://play.qobuz.com) — see [docs/05-qobuz/oauth-and-tokens.ru.md](docs/05-qobuz/oauth-and-tokens.ru.md).

## Docker (preview)

Example compose (no secrets):

```bash
cp docs/04-deployment/compose.example.yml compose.yml
# set EUTERPE_QOBUZ_USER_ID and EUTERPE_QOBUZ_AUTH_TOKEN in .env
docker compose up -d
```

See [docs/04-deployment/docker.ru.md](docs/04-deployment/docker.ru.md).

## Reference projects

Qobuz integration is informed by community tools (unofficial API):

- [qobuz-dl](https://github.com/vitiko98/qobuz-dl)
- [streamrip](https://github.com/nathom/streamrip)
- [qobuz-sync](https://github.com/trevorstarick/qobuz-sync)

## Disclaimer

Requires an active **Qobuz subscription**. This project is not affiliated with Qobuz. Use in compliance with [Qobuz API Terms of Use](https://static.qobuz.com/apps/api/QobuzAPI-TermsofUse.pdf). For personal / educational use.

## License

Licensed under the [Apache License, Version 2.0](LICENSE) (`Apache-2.0`). See [NOTICE](NOTICE) for attribution.

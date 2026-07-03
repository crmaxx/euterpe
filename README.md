# Euterpe (Ἐυτέρπη)

[![CI](https://github.com/crmaxx/euterpe/actions/workflows/ci.yml/badge.svg)](https://github.com/crmaxx/euterpe/actions/workflows/ci.yml)

Euterpe is a self-hosted music library manager with Qobuz sync, playback,
metadata editing, cover handling, torrent import, CUE splitting, conversion
jobs, and storage backends for both local filesystems and SMB shares.

The project is named after Euterpe, the Greek muse of music and lyric poetry.

## Features

- Scan and browse a music library stored on local disk or an SMB share.
- Stream tracks through HTTP range requests.
- Link Qobuz in the web UI and sync/download albums with encrypted credentials.
- Read and write tags, covers, CUE metadata, and integration results through the
  configured storage backend.
- Import completed torrent downloads into the configured library storage.
- Run converter and CUE split jobs without a local library temp bridge.
- Watch configured storage for changes, including SMB ChangeNotify where supported.

## Status

Euterpe is under active development. SMB storage is being promoted to a
first-class library backend, so deployment docs should be treated as
preview-level until the storage migration settles.

## Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/euterpe-server` | Axum API server, workers, routes, storage integration |
| `crates/euterpe-data` | Welds-backed SQLite data layer: connection, migrations, repositories, fixtures |
| `crates/euterpe-qobuz` | Qobuz API client |
| `crates/euterpe-smb` | SMB storage backend wrapper |
| `crates/euterpe-cue` | CUE parsing and split support |
| `crates/euterpe-converter` | Audio conversion pipeline |
| `frontend` | React/Vite web UI |
| `openapi` | OpenAPI schema used to generate the frontend API types |
| `docs` | Architecture, deployment, Qobuz, SMB, and planning docs |
| `third_party/smb-0.11.2` | Vendored SMB crate patched for this integration |

## Requirements

- Rust 1.95 or newer.
- Node.js `>=22.13.0`.
- SQLite, via the bundled Rust dependencies.
- Optional: Docker and Overmind for deployment/dev orchestration.

## Quick Start

```bash
make prepare
cp .env.example .env
make dev
```

The development server starts:

- API: <http://127.0.0.1:8080>
- UI: <http://127.0.0.1:5173>

The Vite dev server proxies `/api` to the backend. Stop both processes with:

```bash
make dev-stop
```

## Configuration

Euterpe reads `.env` from the working directory at server startup. The most
important settings are:

| Variable | Purpose |
| --- | --- |
| `EUTERPE_BIND` | API listen address. Defaults to `127.0.0.1:8080`. |
| `EUTERPE_DATABASE_URL` | SQLite URL. Defaults to a local data path in development. |
| `EUTERPE_MASTER_KEY` | 32-byte hex/base64 key used to encrypt Qobuz, SMB, and integration secrets. Required before saving secrets. |
| `EUTERPE_PUBLIC_BASE_URL` | Public URL used for OAuth redirects. |
| `EUTERPE_STATIC_DIR` | Built frontend directory for production serving. Defaults to `frontend/dist`. |
| `EUTERPE_TORRENT_INCOMING_DIR` | Local incoming directory for torrent inspect/download flows. |
| `EUTERPE_ADMIN_PASSWORD` | Optional UI/API password gate. |

Library storage is configured in the web UI under Settings. Use local storage
for a filesystem path or SMB storage for a network share. `/data`,
`EUTERPE_DATABASE_URL`, and torrent incoming storage remain deployment-managed;
the music library itself should be selected through Settings.

To generate a master key:

```bash
openssl rand -hex 32
```

## Qobuz

Link Qobuz in the web UI: Settings -> Connect Qobuz. The server stores the
encrypted user auth token in SQLite, so `EUTERPE_MASTER_KEY` must be configured
first.

See [docs/05-qobuz/oauth-and-tokens.ru.md](docs/05-qobuz/oauth-and-tokens.ru.md)
for current authentication notes.

## Development

All implementation work follows strict TDD. See
[ADR 0004](docs/adr/0004-test-driven-development.md).

Common commands:

```bash
make backend             # run API server
make frontend            # install, generate API types, run Vite
make dev                 # run backend + frontend through Overmind
make test                # backend + frontend tests
make test-backend        # cargo test --workspace
make test-frontend       # generate API types + frontend tests
```

Frontend API types are generated from `openapi/openapi.yaml`:

```bash
cd frontend
npm run generate:api
```

When `frontend/` or `openapi/` changes, the git hook runs API generation and
ESLint checks to match CI.

## Testing

```bash
cargo test --workspace
cd frontend && npm run generate:api && npm run lint && npm run test && npm run build
```

SMB integration tests that require a real share are ignored by default and gated
by `EUTERPE_TEST_SMB_*` environment variables. Normal CI uses test hooks and
unit tests for SMB path parsing, connection reuse, resource cleanup, and error
mapping.

## Docker

```bash
cp docs/04-deployment/compose.example.yml compose.yml
# set EUTERPE_MASTER_KEY and EUTERPE_PUBLIC_BASE_URL locally
docker compose up -d
```

See [docs/04-deployment/docker.ru.md](docs/04-deployment/docker.ru.md).

## Documentation

- [Documentation index](docs/README.md)
- [Russian documentation index](docs/README.ru.md)
- [Architecture](docs/01-architecture/system-context.ru.md)
- [Docker deployment](docs/04-deployment/docker.ru.md)
- [Qobuz notes](docs/05-qobuz/README.ru.md)
- [SMB storage plans](docs/smb/README.md)

Most detailed documentation is currently written in Russian.

## Reference Projects

Qobuz integration is informed by community projects using the unofficial API:

- [qobuz-dl](https://github.com/vitiko98/qobuz-dl)
- [streamrip](https://github.com/nathom/streamrip)
- [qobuz-sync](https://github.com/trevorstarick/qobuz-sync)

## Disclaimer

Euterpe requires an active Qobuz subscription for Qobuz features. This project
is not affiliated with Qobuz. Use it in compliance with the
[Qobuz API Terms of Use](https://static.qobuz.com/apps/api/QobuzAPI-TermsofUse.pdf).

## License

Licensed under the [Apache License, Version 2.0](LICENSE) (`Apache-2.0`). See
[NOTICE](NOTICE) for attribution.

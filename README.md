# Euterpe (Ἐυτέρπη)

Self-hosted web application for managing a local music library with **Qobuz** synchronization.

Named after the muse of music and lyric poetry, daughter of Mnemosyne.

## Documentation

- **Russian (full):** [docs/README.ru.md](docs/README.ru.md)
- **English (index):** [docs/README.md](docs/README.md)

## Development

All implementation follows **strict Test-Driven Development (TDD)** — see [ADR 0004](docs/adr/0004-test-driven-development.md).

## Stack

| Layer | Technology |
|-------|------------|
| Qobuz client | Rust crate `euterpe-qobuz` (reqwest, rustls) |
| API server | Axum, SQLite (WAL), sqlx |
| UI | Vite, React, Tailwind, TanStack Query/Table, shadcn/ui |
| Deploy | Docker (`/data` + `/music` volumes) |

## Roadmap (short)

1. **Phase 0** — Documentation (current)
2. **Phase 1** — `euterpe-qobuz` library (TDD)
3. **Phase 2** — Axum + SQLite + favorites sync API
4. **Phase 3** — Download jobs + SSE
5. **Phase 4** — React UI
6. **Phase 5** — Tags, covers, library rescan

Details: [docs/00-overview/roadmap.ru.md](docs/00-overview/roadmap.ru.md)

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

TBD (MIT OR Apache-2.0 suggested for workspace)

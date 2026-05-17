# Euterpe Documentation (English index)

**Euterpe** (Ἐυτέρπη) — a self-hosted web app for managing a local music library with Qobuz sync.

Full documentation is written in **Russian** (`.ru.md`). This page is the navigation hub.

## Development policy

All implementation work follows **strict TDD** (Test-Driven Development). See [ADR 0004](adr/0004-test-driven-development.md) and [development-process.ru.md](00-overview/development-process.ru.md).

## Quick links

| Section | Description |
|---------|-------------|
| [README.ru.md](README.ru.md) | Main documentation index (RU) |
| [Vision](00-overview/vision.ru.md) | Product vision and goals |
| [Roadmap](00-overview/roadmap.ru.md) | Phased delivery plan |
| [Future plans](00-overview/future-plans.ru.md) | In-app OAuth, multi Qobuz account switch |
| [Architecture](01-architecture/system-context.ru.md) | System context and containers |
| [Qobuz API](05-qobuz/README.ru.md) | Reverse-engineered API reference |
| [euterpe-qobuz crate](06-library-euterpe-qobuz/README.ru.md) | Rust client library specification |
| [Iteration 1](07-iteration-1/scope.ru.md) | First delivery scope |
| [Docker](04-deployment/docker.ru.md) | Container deployment |

## Stack (summary)

- **Backend:** Rust, Axum, reqwest (rustls), SQLite (WAL), sqlx
- **Frontend:** Vite, React, Tailwind, TanStack Query/Table, shadcn/ui
- **Distribution:** Docker with `/data` and `/music` volumes
- **First code:** `crates/euterpe-qobuz` (Qobuz API client)

## Qobuz authentication (2026)

Automated **email/password login no longer works** reliably; use **`user_auth_token`** from the browser or OAuth. See [oauth-and-tokens.ru.md](05-qobuz/oauth-and-tokens.ru.md).

## Legal

Qobuz integration uses an **unofficial**, reverse-engineered API. Use only with an active Qobuz subscription and in compliance with [Qobuz API Terms of Use](https://static.qobuz.com/apps/api/QobuzAPI-TermsofUse.pdf). This project is not affiliated with Qobuz.

## Reference projects

- [qobuz-dl](https://github.com/vitiko98/qobuz-dl)
- [streamrip](https://github.com/nathom/streamrip)
- [qobuz-sync](https://github.com/trevorstarick/qobuz-sync)

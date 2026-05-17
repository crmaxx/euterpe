# ADR 0001: SQLite with WAL mode

## Status

Accepted

## Context

Single homelab instance, one primary user, catalog + jobs + Qobuz sync metadata. No need for separate DB server on day one.

## Decision

Use SQLite with:

- `PRAGMA journal_mode=WAL`
- `PRAGMA foreign_keys=ON`
- `PRAGMA busy_timeout=5000`
- Single process writer (Axum + in-process Tokio jobs)
- sqlx migrations; schema avoids SQLite-only types where possible for future Postgres

## Consequences

- Simple backup: copy `library.db` (+ checkpoint WAL)
- Not suitable for multiple write-heavy API replicas without migration

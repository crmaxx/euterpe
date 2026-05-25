# ADR 0002: Files as source of truth

## Status

Accepted

## Context

Music library longevity depends on portable files, not a proprietary DB blob store.

## Decision

- Audio files and embedded/sidecar covers in configured library storage are authoritative.
- SQLite stores index, Qobuz IDs, job state, settings.
- Full library can be rebuilt via filesystem rescan (Phase 5).
- Do not store cover BLOBs in DB by default; store `cover_path` optional.

## Consequences

- DB loss is recoverable from disk + Qobuz resync
- Tag edits must write to files first, then update index

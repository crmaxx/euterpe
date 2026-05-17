# ADR 0003: Implement euterpe-qobuz first

## Status

Accepted

## Context

Qobuz integration is the riskiest and most researched part. Axum/UI without a proven client duplicates effort.

## Decision

1. Document API in `docs/05-qobuz`
2. Implement `crates/euterpe-qobuz` with strict TDD
3. Only then wire `euterpe-server` and UI

Server must depend on `euterpe-qobuz`, not embed HTTP logic inline.

## Consequences

- Phase 1 delivers a reusable library (potential future publish to crates.io)
- Delays visible UI until Phase 4

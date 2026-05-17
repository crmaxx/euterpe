# ADR 0004: Strict Test-Driven Development

## Status

Accepted

## Context

Euterpe integrates with an unofficial, reverse-engineered Qobuz API. Behavior can change without notice. The codebase will grow across Rust (client, server, jobs) and TypeScript (UI). Regressions are costly.

## Decision

All feature work **must** follow strict TDD:

1. Write a failing automated test that specifies the behavior.
2. Implement the minimum code to pass.
3. Refactor while keeping tests green.
4. Do not merge code without tests for new behavior.

This applies from Phase 1 (`euterpe-qobuz`) onward, including HTTP handlers and (later) React components.

## Consequences

### Positive

- Signing logic and pagination are verifiable via golden tests.
- Refactors to `QobuzClient` remain safe.
- Documentation and tests stay aligned.

### Negative

- Slower initial velocity for greenfield modules.
- Requires discipline; live Qobuz tests stay `#[ignore]` and manual.

## Implementation notes

- Unit tests: `signing`, `pagination`, JSON deserialization
- HTTP mocks: `mockito` or `wiremock` in dev-dependencies
- Integration: `#[ignore]` + `EUTERPE_QOBUZ_*` env vars
- Frontend: Vitest when UI phase starts

## References

- [development-process.ru.md](../00-overview/development-process.ru.md)
- [testing.ru.md](../06-library-euterpe-qobuz/testing.ru.md)

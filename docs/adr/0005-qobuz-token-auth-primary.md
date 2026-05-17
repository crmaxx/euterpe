# ADR 0005: Qobuz token-first authentication

## Status

Accepted (2026-05)

## Context

Qobuz has moved web login to OAuth with bot protection. Community tools report `user/login` with email/password returns 401 since approximately April 2026 ([qobuz-dl #329](https://github.com/vitiko98/qobuz-dl/issues/329), [streamrip #954](https://github.com/nathom/streamrip/issues/954)).

Existing working approaches:

- `user_id` + `user_auth_token` on `user/login` (streamrip `use_auth_token`)
- Header-only session with pre-copied UAT (qobuz-sync)
- OAuth CLI ([qobuz-dl-go](https://github.com/Aeneaj/qobuz-dl-go))

## Decision

Euterpe uses **token-first** authentication:

1. **Default:** `SessionToken` — `EUTERPE_QOBUZ_USER_ID` + `EUTERPE_QOBUZ_AUTH_TOKEN`, no password.
2. **Optional:** `TokenLogin` — same credentials via `user/login` GET for refresh/validation.
3. **Deprecated:** `EmailPassword` — not exposed in UI; tests only with 401 + user message.
4. **Later:** OAuth callback in `euterpe-server` (Phase 2).

Bundle bootstrap (`app_id`, secrets) remains required for signed download URLs.

## Consequences

- Users must refresh UAT periodically from browser or OAuth.
- Docker docs and Settings UI document token extraction, not password.
- M1 implementation plan tests SessionToken before legacy login.

## References

- [oauth-and-tokens.ru.md](../05-qobuz/oauth-and-tokens.ru.md)
- [authentication.ru.md](../05-qobuz/authentication.ru.md)

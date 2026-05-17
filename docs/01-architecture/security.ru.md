# Безопасность

## Модель угроз (homelab)

- Сеть: доверенный LAN; не expose в интернет без VPN
- Атакующий в LAN может получить доступ к UI — минимальная auth Phase 1
- Утечка UAT / password = доступ к Qobuz от имени пользователя

## Хранение секретов

| Secret | Хранение |
|--------|----------|
| Qobuz user_auth_token (UAT) | **Primary** — env or encrypted SQLite; rotate from browser/OAuth |
| Qobuz user_id | Stored with UAT |
| Qobuz password | **Deprecated** — do not store; API login unreliable since ~2026 |
| app_id, secrets | Cache file in `/data`, not in git |

Не логировать: password, UAT, request_sig inputs with secret.

## Transport

- Qobuz: HTTPS only (reqwest rustls)
- UI → API: HTTP в LAN OK; TLS через reverse proxy optional

## Auth Phase 1

- Single-user password в env `EUTERPE_ADMIN_PASSWORD` → session cookie
- Или rely on network isolation only (документировать риск)

## Auth Phase 2+

- httpOnly Secure cookie
- CSRF для mutating routes
- Rate limit на login

## Docker

- Run as non-root user in container
- Read-only root filesystem where possible
- Cap drop ALL

## Dependencies

- `cargo deny` / `cargo audit` в CI (Phase 2)
- Pin reqwest, sqlx versions

## Legal

Пользователь несёт ответственность за соблюдение ToS Qobuz.

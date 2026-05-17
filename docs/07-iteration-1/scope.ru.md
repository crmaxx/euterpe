# Итерация 1 — Scope

## Цель

Документация + рабочий crate **`euterpe-qobuz`** с покрытием TDD, без полного UI.

## В scope

| Deliverable | Критерий готовности |
|-------------|---------------------|
| `docs/` | Полное дерево, RU + EN index |
| `euterpe-qobuz` M1–M5 | `cargo test -p euterpe-qobuz` green |
| Live tests | `#[ignore]`, documented env |
| ADR | 0001–0004 включая **TDD** |
| compose.example.yml | Template без секретов |

## Вне scope

- `euterpe-server` binary
- React UI
- File download to `/music`
- Tag editing
- Postgres

## Функциональные возможности библиотеки

1. Bootstrap `app_id` + valid `secret`
2. Auth via **user_id + user_auth_token** (SessionToken); email/password deprecated
3. List favorites (albums; tracks/artists optional M2)
4. Create/delete favorites albums
5. `track/getFileUrl` для всех quality levels
6. `album/get`, `artist/get` paginated

## Процесс: строгий TDD

Запрещено: реализация milestone без failing test.

См. [implementation-plan.ru.md](../06-library-euterpe-qobuz/implementation-plan.ru.md).

## Следующая итерация

Phase 2: Axum + SQLite + `POST /qobuz/sync` using `euterpe-qobuz` — также TDD.

## Риски

| Риск | Митигация |
|------|-----------|
| Favorites signing varies | `FavoritesSignMode` fallback + live test |
| Qobuz API change | Fixtures + ignored integration weekly |
| bundle.js parse break | Commit HTML fixture fragment; alert on M1 fail |

## Ссылки

- [05-qobuz](../05-qobuz/README.ru.md)
- [06-library-euterpe-qobuz](../06-library-euterpe-qobuz/README.ru.md)

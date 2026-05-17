# Crate `euterpe-qobuz`

Асинхронный Rust-клиент неофициального Qobuz JSON API. Первый кодовый артефакт проекта Euterpe.

## Границы ответственности

| Входит | Не входит |
|--------|-----------|
| Bootstrap app_id / secrets | SQLite, Axum |
| Login, UAT | Запись файлов на диск (optional feature `download`) |
| Favorites get/create/delete | Tag editing |
| Catalog metadata | UI |
| `track/getFileUrl` | Job queue |

## Зависимости

- `reqwest` + **rustls** (no native-tls)
- `tokio`
- `serde` / `serde_json`
- `thiserror`, `tracing`
- `md-5` (request signatures only)

## Разработка: строгий TDD

Каждый milestone в [implementation-plan.ru.md](implementation-plan.ru.md) начинается со **списка тестов**. См. [testing.ru.md](testing.ru.md) и [ADR 0004](../adr/0004-test-driven-development.md).

## Документы

- [crate-design.ru.md](crate-design.ru.md) — модули и public API
- [types.ru.md](types.ru.md) — serde модели
- [implementation-plan.ru.md](implementation-plan.ru.md) — M1–M5
- [testing.ru.md](testing.ru.md) — стратегия тестов

## Спецификация API

[docs/05-qobuz/](../05-qobuz/README.ru.md)

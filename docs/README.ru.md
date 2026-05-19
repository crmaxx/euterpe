# Документация Euterpe

**Euterpe** (Эвтерпа) — муза музыки и лирической поэзии, дочь Мнемозины. Имя выбрано для self-hosted приложения управления локальной медиатекой с синхронизацией Qobuz.

## Принцип разработки: строгий TDD

Вся реализация (начиная с crate `euterpe-qobuz`) ведётся **строго через TDD**:

1. Написать падающий тест (unit / integration / golden).
2. Минимальная реализация до зелёного теста.
3. Рефакторинг без изменения поведения.
4. Коммит только с зелёным `cargo test` (и позже — frontend tests).

Подробности: [development-process.ru.md](00-overview/development-process.ru.md), [ADR 0004](adr/0004-test-driven-development.md).

## Оглавление

### 00 — Обзор

- [vision.ru.md](00-overview/vision.ru.md) — видение и цели
- [glossary.ru.md](00-overview/glossary.ru.md) — термины
- [roadmap.ru.md](00-overview/roadmap.ru.md) — дорожная карта
- [future-plans.ru.md](00-overview/future-plans.ru.md) — OAuth в приложении, multi-account Qobuz, очередь, FP-4…FP-6 (теги и обложка из UI)
- [development-process.ru.md](00-overview/development-process.ru.md) — TDD и quality gates

### 01 — Архитектура

- [system-context.ru.md](01-architecture/system-context.ru.md)
- [monorepo-layout.ru.md](01-architecture/monorepo-layout.ru.md)
- [data-flow.ru.md](01-architecture/data-flow.ru.md)
- [security.ru.md](01-architecture/security.ru.md)

### 02 — Backend

- [axum-server.ru.md](02-backend/axum-server.ru.md)
- [sqlite-schema.ru.md](02-backend/sqlite-schema.ru.md)
- [migrations.ru.md](02-backend/migrations.ru.md)
- [job-queue.ru.md](02-backend/job-queue.ru.md)

### 03 — Frontend

- [stack.ru.md](03-frontend/stack.ru.md)
- [screens.ru.md](03-frontend/screens.ru.md)
- [api-client.ru.md](03-frontend/api-client.ru.md) — **DRAFT**

### 04 — Деплой

- [docker.ru.md](04-deployment/docker.ru.md)
- [cross-compile.ru.md](04-deployment/cross-compile.ru.md)
- [compose.example.yml](04-deployment/compose.example.yml)
- [backup-restore.ru.md](04-deployment/backup-restore.ru.md)

### 05 — Qobuz API

Локальные зеркала сторонних клиентов (qobuz-dl, streamrip, …): каталог **`docs/references/`** (в `.gitignore`). Сводка путей и auth: [oauth-and-tokens.ru.md](05-qobuz/oauth-and-tokens.ru.md), [reference-implementation.ru.md](05-qobuz/reference-implementation.ru.md).

- [README.ru.md](05-qobuz/README.ru.md)
- [api-reference.ru.md](05-qobuz/api-reference.ru.md)
- [authentication.ru.md](05-qobuz/authentication.ru.md)
- [oauth-and-tokens.ru.md](05-qobuz/oauth-and-tokens.ru.md)
- [request-signing.ru.md](05-qobuz/request-signing.ru.md)
- [quality-formats.ru.md](05-qobuz/quality-formats.ru.md)
- [favorites.ru.md](05-qobuz/favorites.ru.md)
- [pagination.ru.md](05-qobuz/pagination.ru.md)
- [errors.ru.md](05-qobuz/errors.ru.md)
- [reference-implementation.ru.md](05-qobuz/reference-implementation.ru.md)

### 06 — Библиотека `euterpe-qobuz`

- [README.ru.md](06-library-euterpe-qobuz/README.ru.md)
- [crate-design.ru.md](06-library-euterpe-qobuz/crate-design.ru.md)
- [types.ru.md](06-library-euterpe-qobuz/types.ru.md)
- [implementation-plan.ru.md](06-library-euterpe-qobuz/implementation-plan.ru.md)
- [testing.ru.md](06-library-euterpe-qobuz/testing.ru.md)

### 07 — Итерация 1

- [scope.ru.md](07-iteration-1/scope.ru.md)

### ADR

- [0001-sqlite-wal.md](adr/0001-sqlite-wal.md)
- [0002-files-as-source-of-truth.md](adr/0002-files-as-source-of-truth.md)
- [0003-euterpe-qobuz-first.md](adr/0003-euterpe-qobuz-first.md)
- [0004-test-driven-development.md](adr/0004-test-driven-development.md)
- [0005-qobuz-token-auth-primary.md](adr/0005-qobuz-token-auth-primary.md)

## English

Short index: [README.md](README.md).

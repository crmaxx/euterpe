# Процесс разработки: строгий TDD

## Политика

Проект Euterpe разрабатывается **исключительно через Test-Driven Development (TDD)**. Это обязательное правило для:

- crate `euterpe-qobuz`
- `euterpe-server` (Axum)
- фоновых воркеров и миграций
- React UI (Vitest + Testing Library)

Исключения не допускаются без нового ADR и явного согласования.

## Цикл Red → Green → Refactor

```
1. RED    — тест описывает желаемое поведение; тест падает.
2. GREEN  — минимальный код, чтобы тест прошёл.
3. REFACTOR — улучшение структуры; тесты остаются зелёными.
```

### Что считается «тестом первым»

| Слой | Инструмент | Пример |
|------|------------|--------|
| Подписи MD5 | unit + golden vectors | `signing.rs` vs эталон из qobuz-sync |
| HTTP клиент | mockito / wiremock | `user/login` 401/200 |
| Пагинация | unit на фикстурах JSON | `pagination.rs` |
| API handlers | axum + reqwest test | `POST /api/v1/qobuz/sync` |
| OpenAPI spec | Redocly lint | `openapi/openapi.yaml` |
| REST contract | jsonschema vs spec | `openapi_contract.rs` |
| UI | Vitest | кнопка «Синхронизировать» вызывает mutation |

Запрещено: «сначала написать модуль, потом добавить тесты в конце спринта».

## OpenAPI-first (REST API)

См. [openapi-first.ru.md](../02-backend/openapi-first.ru.md) и [ADR 0006](../adr/0006-openapi-first.md).

Изменение REST API:

1. Обновить `openapi/openapi.yaml`.
2. Обновить contract-тест и handler.
3. Обновить [api-client.ru.md](../03-frontend/api-client.ru.md) при изменении примеров.

PR без diff в `openapi.yaml` при изменении JSON-контракта не мержится.

## Quality gates

Перед merge / перед переходом к следующему milestone:

- `cargo test --workspace` — все зелёные
- `npx @redocly/cli lint openapi/openapi.yaml` — spec валиден
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --check`
- Integration-тесты с `#[ignore]` не блокируют CI, но обязательны локально перед релизом Qobuz-фич

## Rust: Clippy

CI и локальная проверка: `-D warnings`. Типичные ловушки (`let_underscore_future`, `mpsc::send` vs `broadcast::send`, таблица частых lint’ов) — в правиле Cursor **`rust-best-practices`** (`~/.cursor/rules/rust-best-practices.mdc`).

## Milestones и TDD

Каждый milestone в [implementation-plan.ru.md](../06-library-euterpe-qobuz/implementation-plan.ru.md) начинается с **списка тестов**, которые должны пройти (Definition of Done = тесты + документация).

Пример M1:

1. Тест: `bundle` парсит mock HTML → `app_id` длиной 9 цифр
2. Тест: `login` с mock 401 → `QobuzError::Authentication`
3. Реализация `bundle.rs`, `api/auth.rs`

## Live API

Тесты против реального Qobuz:

- Маркер `#[ignore]` + env `EUTERPE_QOBUZ_EMAIL` / `EUTERPE_QOBUZ_PASSWORD`
- Не в default CI
- Запуск: `cargo test -p euterpe-qobuz -- --ignored`

## Документация и TDD

Изменение поведения API Qobuz:

1. Обновить `docs/05-qobuz/`
2. Обновить/добавить тест (fixture или golden)
3. Изменить код

См. [ADR 0004](../adr/0004-test-driven-development.md).

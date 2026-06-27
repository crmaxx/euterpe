# Миграции данных

## Инструмент

- Владелец миграций: crate `euterpe-data`.
- Runtime entrypoint: `euterpe_data::migrations::migrate(&DataHandle)`.
- Миграции описываются через Welds migration API и выполняются до запуска HTTP server/workers.
- Старые SQL-файлы в `migrations/` остаются только как compatibility input для уже существующей SQLite-схемы.

## Правила

1. Одна миграция — одна логическая цель.
2. Новая persistence-логика добавляется через `euterpe-data` models/repositories.
3. Server routes, services, workers и tests не создают raw database operations напрямую.
4. Не удалять колонки или менять смысл существующих nullable/unique полей без отдельного ADR.
5. Существующие SQLite базы должны мигрировать вперёд без destructive reset.

## TDD

Миграции покрываются тестами в `crates/euterpe-data/tests/migrations.rs`:

1. Сначала characterization/compatibility test для ожидаемой схемы или legacy базы.
2. Затем Welds migration step.
3. Проверка поведения через typed repositories/fixtures, не через route/service SQL.

## Backup before migrate

Документировать в [backup-restore.ru.md](../04-deployment/backup-restore.ru.md): stop container → copy db → migrate.

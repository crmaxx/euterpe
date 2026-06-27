# Миграции данных

## Инструмент

- Владелец миграций: crate `euterpe-data`.
- Runtime entrypoint: `euterpe_data::migrations::migrate(&DataHandle)`.
- Миграции описываются через Welds migration builders и выполняются до запуска HTTP server/workers.
- Root SQL migration directory больше не является active input для build, tests или runtime.
- SQLx-era compatibility проверяется binary SQLite fixture в `crates/euterpe-data/tests/fixtures`, без `sqlx::migrate!` и без SQL fixture files.

## Правила

1. Одна миграция — одна логическая цель.
2. Новая persistence-логика добавляется через `euterpe-data` models/repositories.
3. Server routes, services, workers и tests не создают raw database operations напрямую.
4. Не удалять колонки или менять смысл существующих nullable/unique полей без отдельного ADR.
5. Существующие SQLite базы должны мигрировать вперёд без destructive reset.

## TDD

Миграции покрываются тестами в `crates/euterpe-data/tests/migrations.rs`:

1. Сначала characterization/compatibility test для ожидаемой схемы или legacy базы.
2. Затем Welds migration builder step.
3. Проверка поведения через typed repositories/fixtures, не через route/service SQL.

## Ограничения Welds builder

Текущая версия Welds builders не покрывает все SQLite DDL-конструкции из старой SQLx-цепочки без возврата к raw SQL:

- partial unique index для active `convert_jobs`;
- composite primary key `scan_keep_paths(scan_id, path)`;
- multi-column unique constraints вроде `qobuz_favorites(entity_type, qobuz_id)`.

Для этого проекта raw SQL в миграциях не используется. Инварианты, которые нельзя выразить builder API, удерживаются typed repository logic и regression tests. Если Welds добавит полноценные builders для этих DDL-форм, миграции можно расширить отдельной builder-only migration.

## Backup before migrate

Документировать в [backup-restore.ru.md](../04-deployment/backup-restore.ru.md): stop container → copy db → migrate.

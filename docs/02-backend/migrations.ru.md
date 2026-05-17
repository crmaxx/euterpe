# Миграции sqlx

## Инструмент

- `sqlx-cli` для create/migrate
- Папка `migrations/` в корне workspace
- `sqlx::migrate!()` в `euterpe-server` startup

## Правила

1. Одна миграция — одна логическая цель
2. Имена: `YYYYMMDDHHMMSS_description.sql`
3. Избегать `AUTOINCREMENT` в комментариях без Postgres плана — использовать INTEGER PK
4. Не удалять колонки без ADR

## TDD

```rust
#[sqlx::test]
async fn migrations_apply(pool: SqlitePool) {
    // pool with migrate
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, 0);
}
```

## Offline / compile-time check

`cargo sqlx prepare` для CI с `DATABASE_URL=sqlite::memory:`.

## Backup before migrate

Документировать в [backup-restore.ru.md](../04-deployment/backup-restore.ru.md): stop container → copy db → migrate.

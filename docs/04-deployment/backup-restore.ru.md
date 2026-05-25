# Бэкап и восстановление

## Что бэкапить

| Артефакт | Критичность |
|----------|-------------|
| `/data/library.db` | Средняя (восстановима rescan + Qobuz sync) |
| `/data/library.db-wal`, `-shm` | При hot backup — см. ниже |
| Library storage | **Высокая** (source of truth) |
| Qobuz credentials в settings | Средняя (можно re-login) |

## Hot backup SQLite (WAL)

### Вариант A — остановка контейнера

```bash
docker compose stop euterpe
cp /var/lib/docker/volumes/euterpe-data/_data/library.db ./backup/
docker compose start euterpe
```

### Вариант B — SQLite backup API

Через `sqlite3` или встроенную команду server Phase 2:

```sql
BACKUP TO '/backup/library.db';
```

### Вариант C — cron

```cron
0 3 * * * docker compose stop euterpe && cp ... && docker compose start euterpe
```

## Restore

1. Stop Euterpe
2. Replace `library.db` from backup
3. Ensure library storage is intact
4. Start Euterpe
5. Run Qobuz sync from UI

## Disaster recovery

Если потерян только DB:

1. Deploy fresh `library.db` via migrations
2. Configure Qobuz credentials
3. `POST /api/v1/library/scan` (Phase 5)
4. `POST /api/v1/qobuz/sync`

Если потеряно хранилище библиотеки — восстановление только из файлового бэкапа; Qobuz re-download via jobs.

## TDD

Backup logic: integration test copy temp db file.

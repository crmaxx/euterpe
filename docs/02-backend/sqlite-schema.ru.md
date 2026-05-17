# Схема SQLite

## Pragmas (при каждом connect)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

## Таблицы

### settings

Key-value для конфигурации.

```sql
CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Ключи (Phase 2, один аккаунт): `qobuz.user_id`, `qobuz.uat_enc`, … Legacy: `qobuz.email` (deprecated).

Ключи (Phase 6+ / FP-2): `qobuz.active_account_id` → `qobuz_accounts.id`.

### qobuz_accounts (future, FP-1 / FP-2)

Несколько привязанных аккаунтов Qobuz; UAT только encrypted.

```sql
CREATE TABLE qobuz_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    label TEXT,                              -- "Мой Studio", опционально
    qobuz_user_id INTEGER NOT NULL UNIQUE,
    uat_encrypted TEXT NOT NULL,             -- ciphertext
    display_name TEXT,
    membership_label TEXT,                   -- Studio, Sublime, ...
    uat_obtained_at TEXT NOT NULL,
    uat_expires_at TEXT,                     -- optional, from JWT exp
    oauth_refresh_encrypted TEXT,            -- optional, FP-1d
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Получение токена: OAuth callback (FP-1) или interim `POST /api/v1/qobuz/accounts` (paste).

### qobuz_favorites

```sql
CREATE TABLE qobuz_favorites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('album', 'track', 'artist')),
    qobuz_id INTEGER NOT NULL,
    title TEXT,
    artist_name TEXT,
    synced_at TEXT NOT NULL,
    removed INTEGER NOT NULL DEFAULT 0,
    -- FP-2: qobuz_account_id INTEGER NOT NULL REFERENCES qobuz_accounts(id),
    UNIQUE (entity_type, qobuz_id)
    -- FP-2: UNIQUE (qobuz_account_id, entity_type, qobuz_id)
);
CREATE INDEX idx_qobuz_favorites_entity ON qobuz_favorites (entity_type, removed);
```

### qobuz_sync_runs

```sql
CREATE TABLE qobuz_sync_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'success', 'failed')),
    albums_added INTEGER DEFAULT 0,
    albums_removed INTEGER DEFAULT 0,
    error_message TEXT
);
```

### download_jobs

```sql
CREATE TABLE download_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    status TEXT NOT NULL CHECK (status IN (
        'queued', 'running', 'completed', 'failed', 'cancelled'
    )),
    job_type TEXT NOT NULL CHECK (job_type IN ('album', 'track', 'artist', 'playlist')),
    qobuz_id INTEGER,
    quality INTEGER NOT NULL DEFAULT 6,
    progress_pct REAL DEFAULT 0,
    payload_json TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_download_jobs_status ON download_jobs (status);
```

### artists

```sql
CREATE TABLE artists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    qobuz_artist_id INTEGER UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### albums

```sql
CREATE TABLE albums (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    artist_id INTEGER REFERENCES artists (id),
    title TEXT NOT NULL,
    year INTEGER,
    qobuz_album_id INTEGER UNIQUE,
    path TEXT UNIQUE,
    cover_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_albums_qobuz ON albums (qobuz_album_id);
```

### tracks

```sql
CREATE TABLE tracks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    album_id INTEGER NOT NULL REFERENCES albums (id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    track_number INTEGER,
    qobuz_track_id INTEGER UNIQUE,
    path TEXT NOT NULL UNIQUE,
    duration_sec INTEGER,
    file_mtime TEXT,
    file_hash TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_tracks_album ON tracks (album_id);
CREATE INDEX idx_tracks_path ON tracks (path);
```

## Postgres-ready rules

- `INTEGER PRIMARY KEY AUTOINCREMENT` → в Postgres migration: `BIGSERIAL`
- `TEXT` datetime → `TIMESTAMPTZ` later
- Без `STRICT` SQLite-only types
- JSON в `payload_json` как TEXT → `JSONB` later

## TDD

sqlx migration test: apply up + down in transaction.

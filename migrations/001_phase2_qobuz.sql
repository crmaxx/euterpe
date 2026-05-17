CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE qobuz_favorites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('album', 'track', 'artist')),
    qobuz_id INTEGER NOT NULL,
    title TEXT,
    artist_name TEXT,
    synced_at TEXT NOT NULL,
    removed INTEGER NOT NULL DEFAULT 0,
    UNIQUE (entity_type, qobuz_id)
);

CREATE INDEX idx_qobuz_favorites_entity ON qobuz_favorites (entity_type, removed);

CREATE TABLE qobuz_sync_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'success', 'failed')),
    albums_total INTEGER DEFAULT 0,
    albums_added INTEGER DEFAULT 0,
    albums_removed INTEGER DEFAULT 0,
    error_message TEXT
);

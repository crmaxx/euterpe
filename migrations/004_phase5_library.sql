CREATE TABLE artists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    qobuz_artist_id INTEGER UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

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
CREATE INDEX idx_albums_artist ON albums (artist_id);

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

CREATE TABLE library_scan_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    status TEXT NOT NULL CHECK (status IN ('running', 'success', 'failed', 'cancelled')),
    files_seen INTEGER NOT NULL DEFAULT 0,
    files_indexed INTEGER NOT NULL DEFAULT 0,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at TEXT,
    error_message TEXT
);

CREATE INDEX idx_library_scan_runs_status ON library_scan_runs (status);

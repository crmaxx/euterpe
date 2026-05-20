CREATE TABLE convert_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'success', 'failed', 'cancelled')),
    trigger TEXT NOT NULL CHECK (trigger IN ('manual', 'auto')),
    files_total INTEGER NOT NULL DEFAULT 0,
    files_done INTEGER NOT NULL DEFAULT 0,
    progress_pct REAL NOT NULL DEFAULT 0,
    error_message TEXT,
    payload_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_convert_jobs_album_status ON convert_jobs (album_id, status);
CREATE INDEX idx_convert_jobs_status ON convert_jobs (status);

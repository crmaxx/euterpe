CREATE TABLE cue_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'success', 'failed')),
    tracks_total INTEGER NOT NULL DEFAULT 0,
    tracks_done INTEGER NOT NULL DEFAULT 0,
    progress_pct REAL NOT NULL DEFAULT 0,
    error_message TEXT,
    payload_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_cue_jobs_album_status ON cue_jobs (album_id, status);
CREATE INDEX idx_cue_jobs_status ON cue_jobs (status);

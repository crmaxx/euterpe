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

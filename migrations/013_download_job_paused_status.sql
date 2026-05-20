-- Extend download_jobs.status CHECK to allow `paused` (SQLite cannot alter CHECK in place).

CREATE TABLE download_jobs_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    status TEXT NOT NULL CHECK (status IN (
        'queued', 'running', 'paused', 'completed', 'failed', 'cancelled'
    )),
    job_type TEXT NOT NULL CHECK (job_type IN ('album', 'track', 'artist', 'playlist', 'torrent')),
    qobuz_id INTEGER,
    quality INTEGER NOT NULL DEFAULT 6,
    progress_pct REAL DEFAULT 0,
    download_speed_bps INTEGER NOT NULL DEFAULT 0,
    queue_position INTEGER NOT NULL DEFAULT 0,
    payload_json TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO download_jobs_new (
    id, status, job_type, qobuz_id, quality, progress_pct, download_speed_bps,
    queue_position, payload_json, error_message, created_at, updated_at
)
SELECT
    id, status, job_type, qobuz_id, quality, progress_pct, download_speed_bps,
    queue_position, payload_json, error_message, created_at, updated_at
FROM download_jobs;

DROP TABLE download_jobs;
ALTER TABLE download_jobs_new RENAME TO download_jobs;

CREATE INDEX idx_download_jobs_status ON download_jobs (status);
CREATE INDEX idx_download_jobs_queue ON download_jobs (job_type, status, queue_position);

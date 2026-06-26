CREATE TABLE IF NOT EXISTS scan_keep_paths (
    scan_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    PRIMARY KEY (scan_id, path)
);

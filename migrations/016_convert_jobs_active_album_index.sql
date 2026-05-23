CREATE UNIQUE INDEX idx_convert_jobs_active_album
    ON convert_jobs (album_id)
    WHERE status IN ('queued', 'running');

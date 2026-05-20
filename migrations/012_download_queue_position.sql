ALTER TABLE download_jobs ADD COLUMN queue_position INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_download_jobs_queue ON download_jobs (job_type, status, queue_position);

-- Backfill queued rows: use id to preserve creation order within each job_type.
UPDATE download_jobs SET queue_position = id WHERE status = 'queued';

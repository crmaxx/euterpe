-- Library scan: total file count after enumerate + jobs handed to index queue.
ALTER TABLE library_scan_runs ADD COLUMN files_total INTEGER NOT NULL DEFAULT 0;
ALTER TABLE library_scan_runs ADD COLUMN files_processed INTEGER NOT NULL DEFAULT 0;

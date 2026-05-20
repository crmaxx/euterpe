-- Hot-reload settings (JSON blobs). INSERT OR IGNORE keeps existing user values.

INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES (
    'ui.preferences',
    '{"theme":"system","locale":"en","default_quality":6}',
    datetime('now')
);

INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES (
    'converter.settings',
    '{"auto_enabled":false,"file_policy":"sibling_then_delete","parallelism":5,"formats":["wav","m4a","ape"],"flac_encode":{"preset":"balanced","block_size":null,"multithread":false}}',
    datetime('now')
);

INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES (
    'library.scan.settings',
    '{"worker_total":10,"enum_workers":5,"process_workers":5,"seed_depth":1,"index_queue_capacity":512,"path_queue_capacity":2048}',
    datetime('now')
);

INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES (
    'downloads.settings',
    '{"concurrency":3}',
    datetime('now')
);

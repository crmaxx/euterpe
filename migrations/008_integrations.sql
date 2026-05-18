CREATE TABLE integrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    type TEXT NOT NULL CHECK (type IN ('tag_source')),
    provider TEXT NOT NULL CHECK (provider IN ('musicbrainz', 'discogs', 'gnudb', 'tracktype')),
    display_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    config_json TEXT NOT NULL DEFAULT '{}',
    config_secrets_enc TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_integrations_type_enabled ON integrations (type, enabled);

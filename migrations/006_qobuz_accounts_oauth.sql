CREATE TABLE qobuz_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    label TEXT,
    qobuz_user_id INTEGER NOT NULL UNIQUE,
    uat_encrypted TEXT NOT NULL,
    display_name TEXT,
    membership_label TEXT,
    uat_obtained_at TEXT NOT NULL,
    uat_expires_at TEXT,
    oauth_refresh_encrypted TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_qobuz_accounts_user ON qobuz_accounts (qobuz_user_id);

CREATE TABLE qobuz_oauth_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    state TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX idx_qobuz_oauth_states_expires ON qobuz_oauth_states (expires_at);

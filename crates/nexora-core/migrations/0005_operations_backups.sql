CREATE TABLE operations (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    instance_id TEXT NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    targets TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE backups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    instance_id TEXT NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    reason TEXT NOT NULL,
    path TEXT NOT NULL,
    size INTEGER NOT NULL
);

CREATE INDEX idx_backups_instance ON backups (instance_id, reason, created_at);

CREATE TABLE instance_contents (
    instance_id TEXT NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    sha1 TEXT NOT NULL,
    sha512 TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime_ms INTEGER NOT NULL,
    provider TEXT NOT NULL DEFAULT 'local',
    project_id TEXT,
    version_id TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    installed_as TEXT NOT NULL DEFAULT 'explicit',
    name TEXT,
    version_number TEXT,
    description TEXT,
    icon_url TEXT,
    local_icon_key TEXT,
    client_side TEXT,
    server_side TEXT,
    categories TEXT NOT NULL DEFAULT '[]',
    dependencies TEXT NOT NULL DEFAULT '[]',
    identified_at INTEGER,
    PRIMARY KEY (instance_id, rel_path)
);

CREATE INDEX idx_instance_contents_kind ON instance_contents (instance_id, kind);

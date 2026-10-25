CREATE TABLE sync_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    categories TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE sync_values (
    profile_id TEXT NOT NULL REFERENCES sync_profiles(id) ON DELETE CASCADE,
    field_id TEXT NOT NULL,
    codec TEXT,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (profile_id, field_id)
);

CREATE TABLE sync_profile_instances (
    instance_id TEXT PRIMARY KEY NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
    profile_id TEXT NOT NULL REFERENCES sync_profiles(id) ON DELETE CASCADE,
    excluded_fields TEXT NOT NULL DEFAULT '[]',
    overrides TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_sync_profile_instances_profile ON sync_profile_instances (profile_id);

CREATE TABLE sync_snapshots (
    instance_id TEXT PRIMARY KEY NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
    applied_revision INTEGER NOT NULL,
    baseline TEXT NOT NULL,
    proposals TEXT NOT NULL DEFAULT '[]',
    state TEXT NOT NULL
);

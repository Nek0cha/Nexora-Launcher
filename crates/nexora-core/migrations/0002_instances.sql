CREATE TABLE instances (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    icon TEXT,
    dir_name TEXT NOT NULL UNIQUE,
    mc_version TEXT NOT NULL,
    loader TEXT NOT NULL DEFAULT 'vanilla',
    loader_version TEXT,
    java_mode TEXT NOT NULL DEFAULT 'auto',
    java_path TEXT,
    memory_mode TEXT NOT NULL DEFAULT 'auto',
    memory_mb INTEGER,
    jvm_args TEXT,
    state TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'created',
    sync_profile_id TEXT,
    created_at INTEGER NOT NULL,
    last_played_at INTEGER,
    play_time_ms INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE java_runtimes (
    path TEXT PRIMARY KEY NOT NULL,
    major_version INTEGER NOT NULL,
    component TEXT,
    os TEXT NOT NULL,
    arch TEXT NOT NULL,
    vendor TEXT,
    source TEXT NOT NULL,
    last_verified_at INTEGER NOT NULL
);

CREATE TABLE running_sessions (
    instance_id TEXT PRIMARY KEY NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
    pid INTEGER NOT NULL,
    process_started_at INTEGER NOT NULL,
    launched_at INTEGER NOT NULL,
    account_id TEXT NOT NULL,
    log_path TEXT NOT NULL
);

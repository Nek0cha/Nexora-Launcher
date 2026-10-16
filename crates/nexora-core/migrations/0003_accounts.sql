CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,          -- UUID（ハイフンなし）
    username TEXT NOT NULL,
    xuid TEXT,
    skin_url TEXT,
    is_active INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL,
    profile_fetched_at INTEGER,
    entitlement_state TEXT NOT NULL CHECK (entitlement_state IN ('verified', 'not_owned', 'unknown')),
    entitlement_checked_at INTEGER,
    verified_at INTEGER,
    needs_login INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX accounts_one_active ON accounts (is_active) WHERE is_active = 1;

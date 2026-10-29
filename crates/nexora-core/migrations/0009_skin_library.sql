-- 保存したスキン。sha256 は画素の内容のハッシュ（skins::pixel_hash）。
CREATE TABLE skin_library (
    id TEXT PRIMARY KEY NOT NULL,          -- UUID（ハイフンなし）
    name TEXT NOT NULL,
    variant TEXT NOT NULL CHECK (variant IN ('classic', 'slim')),
    sha256 TEXT NOT NULL,
    png BLOB NOT NULL,
    added_at INTEGER NOT NULL
);
CREATE INDEX skin_library_sha256 ON skin_library (sha256);

-- 現在のスキンを取り込んだ記録。削除したスキンが戻らないよう、ライブラリとは別に持つ。
CREATE TABLE skin_import_log (
    account_id TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    PRIMARY KEY (account_id, sha256)
);

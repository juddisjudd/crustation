-- What players sent with F7, as Rust posts it to the panel; the screenshot is a JPEG.
CREATE TABLE rust_reports (
    id            TEXT PRIMARY KEY,
    server_id     TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    received_at   TEXT NOT NULL,
    reporter_id   TEXT NOT NULL,
    reporter_name TEXT,
    kind          INTEGER NOT NULL,
    subject       TEXT NOT NULL,
    message       TEXT NOT NULL,
    target_id     TEXT,
    target_name   TEXT,
    image         BLOB
);

CREATE INDEX rust_reports_by_server ON rust_reports (server_id, received_at);

-- Bans every Rust server using the panel's list asks about when somebody joins.
CREATE TABLE rust_shared_bans (
    steam_id   TEXT PRIMARY KEY,
    name       TEXT NOT NULL DEFAULT '',
    reason     TEXT NOT NULL DEFAULT '',
    -- Unix seconds; NULL for a ban that never ends.
    expires_at INTEGER,
    mute       INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    created_by TEXT
);

-- Saved commands, shown as buttons above the console.
CREATE TABLE server_macros (
    id         TEXT PRIMARY KEY,
    server_id  TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    label      TEXT NOT NULL,
    command    TEXT NOT NULL,
    -- Lowest first, so the order is the operator's rather than the clock's.
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX server_macros_server ON server_macros(server_id, position);

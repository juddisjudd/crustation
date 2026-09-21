-- What somebody wrote down about an add-on: how to switch it on, what to run
-- when it misbehaves. Keyed by the pack's id rather than its folder, so an
-- update or a reinstall somewhere else keeps the note.
CREATE TABLE pack_notes (
    server_id  TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    pack_uuid  TEXT NOT NULL,
    text       TEXT NOT NULL DEFAULT '',
    -- A JSON array of {label, command}: a short list, written and read whole.
    commands   TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL,
    PRIMARY KEY (server_id, pack_uuid)
);

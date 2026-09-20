-- The add-on the panel installs into a Bedrock world, and the token it
-- authenticates with. Bedrock has no RCON, so this is how chat, positions and
-- the rest reach the panel at all.
CREATE TABLE server_bridge (
    server_id    TEXT PRIMARY KEY REFERENCES servers(id) ON DELETE CASCADE,
    -- Long and random. The add-on sends it on every request; nothing else does.
    token        TEXT NOT NULL UNIQUE,
    installed_at TEXT NOT NULL
);

CREATE INDEX server_bridge_token ON server_bridge(token);

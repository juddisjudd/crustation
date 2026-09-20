-- Accounts and access control

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    email         TEXT,
    language      TEXT NOT NULL DEFAULT 'en',
    is_admin      INTEGER NOT NULL DEFAULT 0,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    last_login_at TEXT,
    last_login_ip TEXT,
    -- Sessions issued before this moment are rejected.
    sessions_valid_from TEXT NOT NULL
);

CREATE TABLE roles (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE COLLATE NOCASE,
    -- Comma-separated global permissions, for example 'CREATE_SERVER,MANAGE_USERS'.
    global_permissions TEXT NOT NULL DEFAULT '',
    mfa_required INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE user_roles (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE role_servers (
    role_id     TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    server_id   TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    -- Comma-separated per-server permissions.
    permissions TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (role_id, server_id)
);

CREATE TABLE api_keys (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    -- Empty means the key inherits everything the user may do.
    global_permissions TEXT NOT NULL DEFAULT '',
    server_permissions TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL,
    last_used_at TEXT
);

-- Left unused in v1; two-factor lands here without a migration of the user table.
CREATE TABLE auth_methods (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    label      TEXT,
    secret     TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT
);

-- Game servers

CREATE TABLE servers (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    kind             TEXT NOT NULL,
    directory        TEXT NOT NULL,
    executable       TEXT NOT NULL DEFAULT '',
    -- Fully rendered command line; the supervisor runs this verbatim.
    command          TEXT NOT NULL DEFAULT '',
    java_binary      TEXT,
    min_memory_mb    INTEGER NOT NULL DEFAULT 1024,
    max_memory_mb    INTEGER NOT NULL DEFAULT 2048,
    java_flags       TEXT NOT NULL DEFAULT '',
    host             TEXT NOT NULL DEFAULT '0.0.0.0',
    port             INTEGER NOT NULL DEFAULT 25565,
    autostart        INTEGER NOT NULL DEFAULT 0,
    autostart_delay  INTEGER NOT NULL DEFAULT 0,
    crash_detection  INTEGER NOT NULL DEFAULT 1,
    stop_command     TEXT NOT NULL DEFAULT 'stop',
    shutdown_timeout INTEGER NOT NULL DEFAULT 60,
    ignored_exits    TEXT NOT NULL DEFAULT '0',
    count_players    INTEGER NOT NULL DEFAULT 1,
    public_status    INTEGER NOT NULL DEFAULT 0,
    log_path         TEXT NOT NULL DEFAULT 'logs/latest.log',
    -- Where the executable came from, so updates know what to fetch.
    provider         TEXT,
    provider_version TEXT,
    created_by       TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE server_stats (
    server_id      TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    at             TEXT NOT NULL,
    cpu_percent    REAL NOT NULL DEFAULT 0,
    memory_bytes   INTEGER NOT NULL DEFAULT 0,
    memory_percent REAL NOT NULL DEFAULT 0,
    players_online INTEGER,
    players_max    INTEGER,
    PRIMARY KEY (server_id, at)
);

CREATE INDEX server_stats_at ON server_stats(server_id, at DESC);

CREATE TABLE server_players (
    server_id  TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    uuid       TEXT,
    first_seen TEXT NOT NULL,
    last_seen  TEXT NOT NULL,
    online     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (server_id, name)
);

-- Backups, schedules, webhooks

CREATE TABLE backups (
    id             TEXT PRIMARY KEY,
    server_id      TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    destination    TEXT NOT NULL,
    excludes       TEXT NOT NULL DEFAULT '',
    max_archives   INTEGER NOT NULL DEFAULT 0,
    compress       INTEGER NOT NULL DEFAULT 1,
    stop_server    INTEGER NOT NULL DEFAULT 0,
    before_command TEXT,
    after_command  TEXT,
    enabled        INTEGER NOT NULL DEFAULT 1,
    is_default     INTEGER NOT NULL DEFAULT 0,
    last_run_at    TEXT,
    last_status    TEXT,
    last_message   TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE TABLE schedules (
    id          TEXT PRIMARY KEY,
    server_id   TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    -- JSON: {"type":"interval","every_seconds":3600} | {"type":"cron","expression":"0 4 * * *"}
    --     | {"type":"after","schedule_id":"…","delay_seconds":300}
    trigger     TEXT NOT NULL,
    -- JSON: {"type":"start"} | {"type":"backup","backup_id":"…"} | {"type":"command","command":"say hi"}
    action      TEXT NOT NULL,
    one_time    INTEGER NOT NULL DEFAULT 0,
    next_run_at TEXT,
    last_run_at TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE webhooks (
    id         TEXT PRIMARY KEY,
    server_id  TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    provider   TEXT NOT NULL,
    url        TEXT NOT NULL,
    events     TEXT NOT NULL DEFAULT '',
    template   TEXT,
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Panel

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE audit_log (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    at        TEXT NOT NULL,
    user_id   TEXT REFERENCES users(id) ON DELETE SET NULL,
    username  TEXT,
    server_id TEXT,
    action    TEXT NOT NULL,
    detail    TEXT,
    address   TEXT
);

CREATE INDEX audit_log_at ON audit_log(at DESC);

CREATE TABLE login_attempts (
    address  TEXT NOT NULL,
    at       TEXT NOT NULL,
    username TEXT
);

CREATE INDEX login_attempts_address ON login_attempts(address, at DESC);

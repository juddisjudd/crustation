# Crustation architecture

A game server control panel: one Rust binary that supervises game server processes and serves a
Svelte web interface. It is a fresh implementation, not a port; `crafty-feature-reference.md`
records what Crafty Controller does, as a feature checklist only.

## Shape

```
crustation (single binary)
├── http          axum: REST under /api/v1, WebSocket at /ws, SPA from the embedded web build
├── auth          argon2 password hashes, signed session cookie, API keys, roles and permissions
├── store         SQLite via sqlx, migrations in /migrations
├── supervisor    one task per game server: spawn, stdin, stdout ring buffer, crash detection
├── stats         host and per-process sampling, server ping, history for charts
├── scheduler     cron and interval jobs: start, stop, restart, backup, command
├── backups       zip archives with excludes, retention, restore
├── providers     server downloads: Vanilla, Paper, Purpur, Fabric, NeoForge, Bedrock
└── events        broadcast bus that fans out to WebSocket subscribers
```

Everything runs in one process. Game servers are child processes, not containers: the panel owns
their stdin/stdout, which keeps the console honest and the deployment a single container.

## Runtime model

- **Supervisor.** Each server gets a task holding its `Child`, a command channel
  (`Start | Stop | Restart | Kill | Stdin(String)`), and a state machine
  (`Stopped → Starting → Running → Stopping → Stopped`, plus `Crashed`). Stdout and stderr are read
  line by line into a bounded ring buffer (the console backlog) and published on the event bus. A
  stop sends the configured stop command, waits `shutdown_timeout`, then escalates to a kill.
- **Crash handling.** An exit that was not requested, and whose code is not in `ignored_exits`,
  marks the server crashed and, when crash detection is on, restarts it with a backoff.
- **Installs.** Creating a server writes the row and returns; a background task then resolves the
  download from the provider's own API, streams it to disk while checking the published hash,
  unpacks or runs an installer where the flavour needs one, writes `eula.txt` and the port, and
  finally stores the start command.
- **Choosing a JVM.** The providers report what each version asks of Java, and the install picks a
  runtime out of those `java` discovers rather than leaving the command saying `java`. Paper states
  a floor it supports anything above, so it gets the newest installed; Mojang names the runtime a
  release was built for, so those stay close to it. Servers also start on the flags their project
  recommends, widened to Aikar's larger G1 regions once the heap passes 12 GB. Progress goes out as `install` events and as console lines, so
  the record survives a reload. A failure leaves the row with an empty start command, which the
  supervisor refuses to launch.
- **Stats.** A sampler reads process CPU and memory through `sysinfo`, pings the server for
  players, MOTD and version, and writes a row per interval. Charts read a downsampled range.
- **Events.** A `tokio::sync::broadcast` bus carries typed events. WebSocket connections subscribe
  to topics and receive only what they are allowed to see; permission is checked per event, not
  only at subscribe time.

## Data

SQLite with `sqlx`, WAL enabled, migrations applied at startup. Tables: `users`, `roles`,
`user_roles`, `role_servers` (per-server permission mask), `api_keys`, `servers`, `server_stats`,
`backups`, `backup_archives`, `schedules`, `webhooks`, `audit_log`, `settings`.

Server IDs are UUIDs. Times are stored as UTC RFC 3339 strings and sent to the client as UTC; the
interface formats them in the viewer's locale.

## Permissions

- Global: `CREATE_SERVER`, `MANAGE_USERS`, `MANAGE_ROLES`, `ADMIN`.
- Per server, granted through roles: `COMMANDS`, `CONSOLE`, `LOGS`, `SCHEDULES`, `BACKUPS`,
  `FILES`, `CONFIG`, `PLAYERS`.

An admin implicitly holds everything. Every endpoint checks the caller's effective permission for
the resource, and the same check gates the events a WebSocket receives.

## Layout on disk

```
/config     crustation.toml, crustation.db, secrets, certificates
/servers    <server-id>/ per game server, plus .uploads/ for archives waiting to be imported
/backups    <server-id>/ archives
```

These are the three Docker volumes, and the three Unraid mappings.

## Deployment

The image is multi-stage: Node builds the interface, Rust builds a static binary, and the runtime
stage carries the JREs game servers need plus the binary. It runs as a non-root user, takes `PUID`
and `PGID` like other Unraid containers, and stores nothing outside the three volumes.
`docker/unraid.xml` is the Community Applications template.

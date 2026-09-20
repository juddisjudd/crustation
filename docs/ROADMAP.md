# Roadmap

Target: everything Crafty Controller does, minus two-factor authentication and passkeys, with
Docker and Unraid supported from the start. `crafty-feature-reference.md` is the checklist.

## Done

- Panel boots, creates the first administrator, writes its own config.
- SQLite schema and migrations for users, roles, servers, backups, schedules, webhooks, audit.
- Sessions (argon2 + signed cookie), API keys, global and per-server permissions.
- Supervisor: spawn, stdin, console ring buffer, ready detection, graceful stop with escalation,
  crash detection and restart, autostart on boot, clean shutdown.
- Host and per-process stats sampling with history and pruning.
- WebSocket with per-connection topics and permission checks on every event.
- REST: auth, server list/detail/update/delete, actions, command, console, panel stats, audit,
  Java discovery.
- Svelte interface on `/api/v1` with topic subscriptions: sign-in, overview, server page, live
  console.
- RCON on Java servers: the panel turns it on in `server.properties`, sends commands over it when
  it is set up, falls back to stdin when it is not, and puts both sides of the exchange in the
  console everyone is watching.
- Docker image (interface + panel + JREs), compose file, Unraid template, entrypoint with
  PUID/PGID.

## Next

1. **Server creation.** Providers for Vanilla, Paper, Purpur, Fabric, NeoForge and Bedrock;
   download with progress, EULA handling, first-run setup; import from a zip or an existing folder.
   Mojang puts the Bedrock download behind bot protection, so that provider also needs a paste-a-URL
   or upload-the-zip path.
2. **Protocol status.** Server List Ping on Java and the RakNet unconnected ping on Bedrock report
   MOTD, version, player count and latency over the wire, including for servers the panel did not
   start; console parsing stays as the fallback.
3. **server.properties editor.** A typed form with descriptions, validation, a diff before apply,
   and a badge on the keys that need a restart. Same file on Java and Bedrock, different keys.
4. **Files.** List, read, write, rename, move, copy, delete, upload with progress, unzip, download.
5. **Backups.** Configs, runs with progress, retention, restore, download, excludes. Taken
   automatically before anything destructive: version changes, content installs, restores.
6. **Schedules.** Interval, cron and chained triggers; run now; next-run calculation. A scheduled
   restart broadcasts a warning ladder in game before it stops the server.
7. **Players.** Online list and history. `ops.json`, `whitelist.json`, `banned-players.json` and
   `banned-ips.json` on Java, `allowlist.json` and `permissions.json` on Bedrock (keyed by XUID),
   all as editable tables with username to UUID lookup and player heads.
8. **Users and roles.** Administration screens and their endpoints, plus invite links that grant a
   scoped per-server role, so an owner can hand a friend console-only access.
9. **Content.** Modrinth search, install, dependency resolution and update checks, disabling by
   rename rather than delete. Bedrock behaviour and resource packs with their two manifests.
   Datapacks. Worlds: switch `level-name`, upload, download, generate from a seed, reset the Nether
   and the End.
10. **Guarded upgrades.** Back up, swap the jar, start, and roll back on its own if the server never
    reaches ready.
11. **Webhooks and alerts.** Discord, Slack, Mattermost, Teams; event triggers; test send. Crash and
    failed-backup notifications, and browser push through the manifest the interface already ships.
12. **Metrics and health.** Range queries with downsampling behind the charts. TPS and MSPT from
    RCON on Paper, with "Can't keep up!" and stack traces lifted out of the console stream.
13. **Public status page**, panel settings, branding.

## Alongside

Unblocked by the list above, and most of what makes the panel feel finished:

- Console: level filter chips, regex search with highlighting, section-sign colour codes, a
  timestamp toggle, download the buffer.
- Saved command macros per server, as buttons.
- Bulk actions across servers, reachable from the command palette.
- JVM flag presets sized to the memory limit, instead of a hand-written command line.
- A warning when two servers claim the same port.
- Audit entries on the server page: who did what, and when.

## Later

- Two-factor authentication and passkeys (the schema already has a place for them).
- A Docker backend for the supervisor, so each game server can run in its own container.
- Non-Minecraft games: Hytale, SteamCMD titles.
- Multi-node: one panel, several machines.

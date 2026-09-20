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
- Docker image (interface + panel + JREs), compose file, Unraid template, entrypoint with
  PUID/PGID.

## Next

1. **Interface port.** The Svelte app currently speaks Crafty's API. Point it at `/api/v1`, swap
   the WebSocket to topic subscriptions, and delete what no longer applies.
2. **Server creation.** Providers for Vanilla, Paper, Purpur, Fabric, NeoForge and Bedrock;
   download with progress, EULA handling, first-run setup; import from a zip or an existing folder.
3. **Files.** List, read, write, rename, move, copy, delete, upload with progress, unzip, download.
4. **Backups.** Configs, runs with progress, retention, restore, download, excludes.
5. **Schedules.** Interval, cron and chained triggers; run now; next-run calculation.
6. **Players.** Online list, history, ban/kick/op through the console; query for the status page.
7. **Users and roles.** Administration screens and their endpoints.
8. **Webhooks.** Discord, Slack, Mattermost, Teams; event triggers; test send.
9. **Metrics.** Range queries with downsampling behind the charts.
10. **Public status page**, panel settings, branding.

## Later

- Two-factor authentication and passkeys (the schema already has a place for them).
- A Docker backend for the supervisor, so each game server can run in its own container.
- Non-Minecraft games: Hytale, SteamCMD titles.
- Multi-node: one panel, several machines.

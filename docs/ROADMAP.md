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
- Server creation: Vanilla, Paper, Purpur, Fabric, NeoForge and Bedrock, resolved from each
  project's own API; download with progress and checksum, the NeoForge installer, unpacking
  Bedrock, EULA, port, and a start command worked out per flavour. Import from an uploaded zip
  (with a picker for the folder inside it), a folder on the host, or a pasted URL. Install runs in
  the background and reports itself over the WebSocket and into the console.
- New servers start on a JVM that suits them: the install reads what the version asks of Java and
  picks from the runtimes on the host, and runs it on the flags its project recommends, sized to
  the memory limit. The image carries Java 8, 11, 17, 21 and 25 to choose between.
- The creation form offers the `server.properties` settings worth choosing up front: the seed and
  world type, difficulty, game mode, slots, the allow list and the view distances. Java and
  Bedrock each get their own keys, checked against a catalogue before anything is written.
- Protocol status: the Java Server List Ping and Bedrock's RakNet unconnected ping, asked of every
  server on each sample, so the MOTD, version, players and latency are what the server itself
  says. Servers the panel did not start answer too. Bedrock only listens for that ping under
  `transport=raknet`, which the creation form now offers, because Mojang ships `nethernet` and it
  opens no port at all.
- A `server.properties` editor on each server: 28 Java keys and 22 Bedrock ones as a typed form
  with help and validation, and every other key in the file as plain text, so nothing is hidden
  and nothing is out of reach. The keys the panel writes itself are shown but locked.
- A file manager on each server: browse, read and edit text in place, create, rename, copy, move,
  delete, upload, download a file or a whole folder as a zip, and unpack an archive where it sits.
  Every path is checked twice, once as text and once after resolving links, so nothing reaches
  outside the server's own folder.
- Docker image (interface + panel + JREs), compose file, Unraid template, entrypoint with
  PUID/PGID.

## Next

1. **Schedules.** Interval, cron and chained triggers; run now; next-run calculation. A scheduled
   restart broadcasts a warning ladder in game before it stops the server.
2. **Players.** Online list and history. `ops.json`, `whitelist.json`, `banned-players.json` and
   `banned-ips.json` on Java, `allowlist.json` and `permissions.json` on Bedrock (keyed by XUID),
   all as editable tables with username to UUID lookup and player heads.
3. **Users and roles.** Administration screens and their endpoints, plus invite links that grant a
   scoped per-server role, so an owner can hand a friend console-only access.
4. **Content.** Modrinth search, install, dependency resolution and update checks, disabling by
   rename rather than delete. Bedrock behaviour and resource packs with their two manifests.
   Datapacks. Worlds: switch `level-name`, upload, download, generate from a seed, reset the Nether
   and the End. Bedrock keeps its experiment toggles, Beta APIs among them, in the world's
   `level.dat` rather than in `server.properties`, so turning those on means reading and writing
   Bedrock's little-endian NBT. Java needs none of that: its experiments arrive as feature packs
   named in `initial-enabled-packs`, which server creation already offers.
5. **Guarded upgrades.** Keep a copy of the old jar, swap it, start, and put the old one back on
   its own if the server never reaches ready. Originally this leaned on the backup manager; with
   backups moved out it keeps its own copy instead, which is all the rollback actually needs.
6. **Webhooks and alerts.** Discord, Slack, Mattermost, Teams; event triggers; test send. Crash and
   failed-backup notifications, and browser push through the manifest the interface already ships.
7. **Metrics and health.** Range queries with downsampling behind the charts. TPS and MSPT from
   RCON on Paper, with "Can't keep up!" and stack traces lifted out of the console stream.
8. **Public status page**, panel settings, branding.

## Alongside

Unblocked by the list above, and most of what makes the panel feel finished:

- Console: level filter chips, regex search with highlighting, section-sign colour codes, a
  timestamp toggle, download the buffer.
- Saved command macros per server, as buttons.
- Bulk actions across servers, reachable from the command palette.
- A warning when two servers claim the same port.
- Audit entries on the server page: who did what, and when.

## Later

- Backups: configs, runs with progress, retention, restore, download, excludes. Judd runs his own
  backups of the whole share, so the panel repeating that work is not worth the weight. The
  `backups` table and the volume stay where they are for whenever it is.
- Two-factor authentication and passkeys (the schema already has a place for them).
- A Docker backend for the supervisor, so each game server can run in its own container.
- Non-Minecraft games: Hytale, SteamCMD titles.
- Multi-node: one panel, several machines.

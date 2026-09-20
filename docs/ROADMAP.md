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
- Users, roles and API keys, with screens for all three. A role carries panel-wide permissions and
  a grid of per-server ones, and a person's rights are the union of the roles they hold. You
  cannot delete, disable or demote yourself, and the last administrator cannot be removed by any
  route. A server nobody granted answers 404 rather than 403, so the list of servers stays
  private. Invite links are still to come.
- A console worth reading: level chips, plain or regex search with the hits highlighted in place,
  section-sign colours rendered as the game means them, a timestamp toggle, and a button to
  download the buffer. Saved commands sit above the input as buttons for everybody who can use
  that console.
- A warning on any server sharing a port with another of the same edition, since Java binds TCP
  and Bedrock binds UDP and only a clash within one of them stops the other starting.
- A players page: who is on, who has been on, and editable tables for each of the files a server
  keeps beside its world. Java's `ops.json`, `whitelist.json`, `banned-players.json` and
  `banned-ips.json`, and Bedrock's `allowlist.json` and `permissions.json`, each written in the
  shape the game expects. Adding somebody to a Java list looks their UUID up with Mojang rather
  than writing half a row, and every face is their own head.
- Docker image (interface + panel + JREs), compose file, Unraid template, entrypoint with
  PUID/PGID.
- Acting on a player from the list: operator on and off, the operator level or Bedrock permission,
  kick, ban and pardon, give an item, teleport to somebody or to a spot, and a quiet word. A
  running server gets the command; a stopped one gets the file, so who is banned and who is an
  operator can be settled with nothing running. Everybody any list names shows in one table with
  their badges, so a ban is one menu rather than four tabs.
- The message of the day, edited where it is read: a palette for the colour codes, the line the
  game will actually show, and a warning when it is longer than the list will fit.
- A chat tab: what people said, who came and went, and what they earned, lifted out of the console
  stream, with a box to talk back as the server. Bedrock writes no chat to its console, and the tab
  says so rather than looking broken.
- The console reads like a log: the timestamp, thread and level dimmed, chat, addresses, quoted
  text and numbers picked out, all of it behind a switch for anyone who would rather it plain.
- Add-ons, installed from the file browser. A .mcaddon, .mcpack or zip is opened, every pack
  inside it found by its manifest, and each one put where the server reads it and switched on
  for the world. A .mcaddon that holds .mcpack files is opened in turn. Java datapacks arrive
  the same way, through their pack.mcmeta.
- Editing a file is its own view with a way back, rather than a panel wedged under the browser.
- Worlds arrive the same way. A .mcworld goes to worlds/<name> on Bedrock, a zipped Java world
  to its own folder beside the jar, the name taken from levelname.txt where there is one, and
  level-name is pointed at it.
- The Bedrock add-on. Bedrock has no RCON and writes no chat to its console, so the panel now
  ships a behaviour pack that posts chat, arrivals, departures, deaths and where everybody is
  standing back to the panel every second. One button installs the pack, tells the world to
  load it, adds the two script modules to the allow list and turns the Beta APIs experiment on
  in level.dat, which meant learning to read and write Bedrock's little-endian NBT. With it on,
  the chat tab and the live map work on Bedrock as they do on Java. The channel only ever
  sends: commands already reach a Bedrock server on its standard input.
- Xbox ids are learned from the console. Bedrock names one when somebody connects and at no
  other time, so the panel writes it down, and can then key permissions.json properly.
- Add-ons and worlds have their own tab and their own buttons. **Install add-on** takes a
  .mcaddon or .mcpack straight from a file dialog, unpacks it into `behavior_packs` or
  `resource_packs` and writes it into that world's `world_behavior_packs.json` or
  `world_resource_packs.json`; where a server keeps more than one world it asks which first.
  **Import world** takes a .mcworld and puts it under `worlds/` under the name the world gives
  itself, without changing what the server is playing, and the list of worlds below has a button
  for that when you mean it. Neither file ever lands in the server folder: it is unpacked from a
  scratch folder outside and deleted either way. Installing from the file browser still works for
  a file already there.
- An MCP server at `/mcp`, so an assistant can look at a server rather than be told about it:
  thirteen tools covering the list, the console, commands, players, `server.properties`, reading
  and writing the files, and the add-ons, plus the same ground as resources. It is the panel's own permissions
  throughout — the key is an API key, a session cookie is refused, and every call goes through
  the check the REST route would have made. Writes land in the audit log marked as MCP.

## Next

1. **Schedules.** Interval, cron and chained triggers; run now; next-run calculation. A scheduled
   restart broadcasts a warning ladder in game before it stops the server.
2. **Content.** Modrinth search, install, dependency resolution and update checks, disabling by
   rename rather than delete. Bedrock behaviour and resource packs with their two manifests.
   Datapacks. Worlds: switch `level-name`, upload, download, generate from a seed, reset the Nether
   and the End. Bedrock keeps its experiment toggles, Beta APIs among them, in the world's
   `level.dat` rather than in `server.properties`, so turning those on means reading and writing
   Bedrock's little-endian NBT. Java needs none of that: its experiments arrive as feature packs
   named in `initial-enabled-packs`, which server creation already offers.
3. **Guarded upgrades.** Keep a copy of the old jar, swap it, start, and put the old one back on
   its own if the server never reaches ready. Originally this leaned on the backup manager; with
   backups moved out it keeps its own copy instead, which is all the rollback actually needs.
4. **Webhooks and alerts.** Discord, Slack, Mattermost, Teams; event triggers; test send. Crash and
   failed-backup notifications, and browser push through the manifest the interface already ships.
5. **Metrics and health.** Range queries with downsampling behind the charts. TPS and MSPT from
   RCON on Paper, with "Can't keep up!" and stack traces lifted out of the console stream.
6. **Public status page**, panel settings, branding.

## Alongside

Unblocked by the list above, and most of what makes the panel feel finished:

- Bulk actions across servers, reachable from the command palette.
- Audit entries on the server page: who did what, and when.

## Later

- Backups: configs, runs with progress, retention, restore, download, excludes. Judd runs his own
  backups of the whole share, so the panel repeating that work is not worth the weight. The
  `backups` table and the volume stay where they are for whenever it is.
- Two-factor authentication and passkeys (the schema already has a place for them).
- A Docker backend for the supervisor, so each game server can run in its own container.
- Non-Minecraft games: Hytale, SteamCMD titles.
- Multi-node: one panel, several machines.

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
- Notes on an add-on, since plenty of them need something run in the game before they do
  anything and the pack is the only place that says so. Free text, plus commands as buttons that
  run the way the console runs one. The panel reads the pack for starting points: the functions
  it ships, the script event ids its scripts watch for, the slash commands they register and the
  settings its manifest declares. Kept per server against the pack's id, so an update keeps them.
- An MCP server at `/mcp`, so an assistant can look at a server rather than be told about it:
  thirteen tools covering the list, the console, commands, players, `server.properties`, reading
  and writing the files, and the add-ons with their notes, plus the same ground as resources. It is the panel's own permissions
  throughout — the key is an API key, a session cookie is refused, and every call goes through
  the check the REST route would have made. Writes land in the audit log marked as MCP.
- Rust, the game. Installed through SteamCMD, which the panel fetches once into the config
  volume and runs with progress reported like any other download. Map, seed, size, slots and PvE
  are chosen at creation and go on the command line, RCON is switched on over the loopback with a
  generated password and used for commands and a clean `quit`, and status comes from the Steam
  query.
- Rust, tried on a live server. The install reports each SteamCMD step (updating itself, signing
  in, making room, the download with its size, checking, moving into place) instead of one
  silent bar. RCON skips the log lines Rust streams down the same socket and the empty packet it
  sends for a silent command, so every command answers in well under a second. Player counts
  come from the `cp` and `mp` query tags, since the byte fields wrap at 255. The settings
  defaults match what the game reports, and the players, add-on and bridge routes refuse a Rust
  server instead of writing Minecraft files into it.
- Rust on websocket RCON, over one socket kept open per server, so the panel's own commands
  never reach the console and Rust's one-second reconnect cooldown never bites. Existing servers
  move over on their next start.
- Rust players: who is on (`playerlist`) and a roster built from it, kick, ban for a while or for
  good, unban, owners and moderators, mute, teleport and give. A stopped server gets the edit in
  `bans.cfg` or `users.cfg` instead, and a ban is saved with `server.writecfg` straight away,
  since Rust otherwise holds it in memory until something else saves.
- Rust chat from `chat.tail`, with a box to talk as the server; chat now uses the console's
  full width on every server.
- Rust wipes: map, blueprints or everything, with an optional new seed, never touching
  `companion.id`, `player.tokens` or `cfg/`.
- Rust updates: the installed build against the newest on its branch, checked every half hour,
  and an update that counts down in chat, stops, runs SteamCMD and starts again.
- Schedules on every server: repeat, cron (both day fields have to match, so "first Thursday" can
  be said) and after another schedule; start, stop, restart with a countdown in chat, or a
  command, and on Rust a wipe or an update, optionally only when Steam has a newer build. Each
  keeps what its last run did. Presets cover a nightly restart, a weekly map wipe, the monthly
  full wipe and updating whenever Facepunch ships.
- Oxide and Carbon on Rust, installed, swapped and taken off from a Plugins tab. Taking Oxide off
  runs Steam's check and deletes Oxide's own files; an update puts Oxide back, since the check
  replaces the assemblies it patched. Carbon goes on through Unity Doorstop on the start command
  and leaves the game alone. Plugins upload, switch off, delete, install and update from uMod,
  and a running framework is told to load or unload each one, since its file watcher cannot be
  relied on across a mounted drive.
- Rust chat arrives as it is said over the open RCON socket, and `serverinfo` feeds a health card
  with frame rate, entities, queue and network.
- More Rust settings: tutorial island, hidden player names, F7 reports in the console, save
  backups, boombox radios, a favourites address, creative mode, the wipe timer's cron line, and the
  Rust+ port and address, with Rust+ published in Docker.
- F7 reports and a shared ban list on Rust. A server can post its reports to the panel, which
  keeps them with their screenshots on the players page, and can ask the panel about every player
  who joins. The list is edited by an administrator only, and a new ban kicks the player off every
  running server that uses it, since Rust only asks at join. Both hooks are answered on the
  loopback alone, which is where a game server in the same container comes from, and the report
  hook also wants the key the panel wrote onto that server's start command.
- A tag picker for Rust's server browser tags, one chip per tag, which keeps to one tag from each
  exclusive group (wipe schedule, difficulty, region); the panel refuses a mix that breaks the
  rule. Rust+ shows on the health card as off, listening, or failed with what Rust said about
  why, since a failed connectivity test otherwise only scrolls past in the console.
- An Admin tab on Rust with three lists from Carbon's reference, or from the running server's own
  `find .` when it answers: convars with their live values, which can be set for now, saved to the
  start command, or taken off it; console commands, searchable, with a short list of the common
  ones and a box to run any of them; and items with their pictures, given to one player or to
  everyone online. The give dialog on the players page picks from the same items.
- A quieter Rust console and the map on the overview. What Unity, Steam and Rust print on every
  start (missing shaders, things created because they were missing, navmesh progress, the command
  line echoed three times) is marked as noise and hidden behind a chip; no convar or plugin can
  stop the game printing it. A line naming a player is never hidden. SteamCMD's output has a chip
  of its own, which leaves its result and its errors in view. The picture Rust uploads of
  its map is read from the console and shown once the server is up. The Admin tab no longer asks
  `find .` during a start, which threw an exception into the console before Steam was ready.
- More to do about a Rust player with no framework installed: heal, heal everyone, revive, kill
  and clear the inventory, each a vanilla console command sent over RCON. Kill and clear ask first.
- A panel for each Rust player, opened from their name: status, time connected, health and
  address, with everything there is to do about them in groups. The row menu stays for the quick
  ones, no longer wraps, and leaves out what cannot be done to somebody who is not on the server.
- A Network card on every server's overview: the local and public address, and each port players
  reach with whether it is open. The panel asks the port on the loopback, then through its own
  public address, so a forwarded port answers the way it would for a player. Silence only reads
  as closed once something has answered that way, since a router that does not loop traffic back
  looks the same as a closed port.
- Enshrouded and RuneScape: Dragonwilds, the second and third games through SteamCMD, which proved
  the Steam install, the Source query and the update check general: one list of Steam titles now
  says what each one is, and the updates card, the update schedule and `/steam/build` serve all
  three. Enshrouded ships for Windows only, so on Linux SteamCMD fetches the Windows build and the
  image carries WineHQ's stable Wine to run it; Dragonwilds runs its own Linux binary. Each is
  configured by its own file (`enshrouded_server.json`, `DedicatedServer.ini`), written at
  creation and edited from the settings tab while the server is stopped, since both games rewrite
  it while they run. Enshrouded's every 1.0 game setting is offered with its range, its roles and
  their join passwords have an editor on the players tab, and its bans can be lifted there.
  Dragonwilds will not start without the owner's Player ID, so the form asks for it up front, and
  its console has the world password taken out of every line that prints it.
- Neither game reads a command, so the console is read-only for them and a stop is Ctrl+C, sent to
  the server's own process group so it reaches the game behind Wine. Enshrouded saves on the way
  out; Dragonwilds saves every five minutes and when a player leaves. Who is on comes from the
  join and leave lines each prints.
- Two first-time SteamCMD installs at once no longer trip over each other fetching SteamCMD, and
  every install asks Steam for fresh app info first, which stops Dragonwilds' "Missing
  configuration" failure.
- `crustation reset-password <username>` lets someone who cannot sign in back in from a shell: a
  new password, printed once, with their sessions ended and the sign-in cooldown lifted. It runs
  beside the live panel, since SQLite lets the two share the database.

## Next

1. **Content.** Modrinth search, install, dependency resolution and update checks, disabling by
   rename rather than delete. Bedrock behaviour and resource packs with their two manifests.
   Datapacks. Worlds: switch `level-name`, upload, download, generate from a seed, reset the Nether
   and the End. Bedrock keeps its experiment toggles, Beta APIs among them, in the world's
   `level.dat` rather than in `server.properties`, so turning those on means reading and writing
   Bedrock's little-endian NBT. Java needs none of that: its experiments arrive as feature packs
   named in `initial-enabled-packs`, which server creation already offers.
2. **Guarded upgrades.** Keep a copy of the old jar, swap it, start, and put the old one back on
   its own if the server never reaches ready. Originally this leaned on the backup manager; with
   backups moved out it keeps its own copy instead, which is all the rollback actually needs.
3. **Webhooks and alerts.** Discord, Slack, Mattermost, Teams; event triggers; test send. Crash and
   failed-backup notifications, and browser push through the manifest the interface already ships.
4. **Metrics and health.** Range queries with downsampling behind the charts. TPS and MSPT from
   RCON on Paper, with "Can't keep up!" and stack traces lifted out of the console stream.
5. **Public status page**, panel settings, branding.

## Rust parity

What the other Rust panels do, in the order worth doing it. Every command named here was checked
against a live server or the Facepunch wiki.

Most of it is done (see above). Still to come:

1. **Custom map hosting.** Players download a custom map from its link, so serving an uploaded
   .map means the panel answering the internet. Waits for `CRUSTATION_PUBLIC_URL` to be set.
2. **A circle preview for the server icon**, with a check on its size.

The branch list is fixed at release, staging and aux02 for now; Steam's own list rotates, so it
should come from `app_info_print`.

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
- Non-Minecraft games: Hytale. Rust, Enshrouded and RuneScape: Dragonwilds are in, and another
  SteamCMD title is an entry in `steam_games.rs` plus its own module.
- A graceful stop for Enshrouded and Dragonwilds on a Windows host. Ctrl+C reaches them on Linux;
  on Windows the stop still waits out the timeout and ends the process.
- Multi-node: one panel, several machines.

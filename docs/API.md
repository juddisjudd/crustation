# Crustation API

Everything the interface uses lives under `/api/v1`. This is the contract: the Rust side implements
it, the Svelte side consumes it, and neither invents shapes the other does not know about.

## Conventions

- Success: `200` (or `201` on create) with `{"status":"ok","data": …}`. `data` is omitted only for
  actions that return nothing.
- Failure: `{"status":"error","error":"CODE","message":"human sentence"}` with a real status code.
  Codes in use: `UNAUTHENTICATED` (401), `FORBIDDEN` (403), `NOT_FOUND` (404), `CONFLICT` (409),
  `VALIDATION` (422, plus `fields: {name: message}`), `RATE_LIMITED` (429, plus `retry_after`),
  `SERVER_ERROR` (500), `UNAVAILABLE` (503).
- Auth is a `crustation_session` cookie: HttpOnly, SameSite=Lax, Secure when served over HTTPS.
  Automation uses `Authorization: Bearer <api key>` instead. Because the cookie is SameSite=Lax and
  the interface is same-origin, there is no CSRF token.
- IDs are UUID strings. Timestamps are RFC 3339 in UTC. Sizes are bytes as numbers; the interface
  formats them.
- List endpoints return arrays, never objects keyed by id.

## Auth

| Method | Path             | Purpose                                                                                                                           |
| ------ | ---------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| POST   | `/auth/login`    | `{username, password}` → `{user}`, sets the session cookie. `401 UNAUTHENTICATED`, `429 RATE_LIMITED` with `retry_after` seconds. |
| POST   | `/auth/logout`   | Clears the session.                                                                                                               |
| GET    | `/auth/session`  | The signed-in user, effective permissions and panel state. `401` when signed out.                                                 |
| POST   | `/auth/password` | `{current_password, new_password}` for the caller.                                                                                |

`GET /auth/session` returns what the shell needs in one call:

```json
{
  "user": {
    "id": "…",
    "username": "admin",
    "email": null,
    "language": "en",
    "is_admin": true
  },
  "permissions": { "global": ["CREATE_SERVER", "…"] },
  "panel": {
    "version": "0.1.0",
    "docker": true,
    "timezone": "Europe/London",
    "starting": false
  }
}
```

## Servers

| Method | Path                    | Purpose                                                                                                                                                                                                                                                                                         |
| ------ | ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET    | `/servers`              | Servers the caller can see, each with `stats`, `flags` and the caller's `permissions`. One call feeds the whole overview.                                                                                                                                                                       |
| POST   | `/servers`              | Create from a provider download or an import. Returns `201 {id}`; the work continues in the background and is reported over the WebSocket. Requires `CREATE_SERVER`.                                                                                                                            |
| GET    | `/servers/{id}`         | One server with its settings, stats, flags and permissions.                                                                                                                                                                                                                                     |
| PATCH  | `/servers/{id}`         | Change settings. Only fields the caller may change are accepted.                                                                                                                                                                                                                                |
| DELETE | `/servers/{id}`         | `?delete_files=true` also removes the folder. The server has to be stopped first, or the answer is `409 CONFLICT`. Requires `CONFIG`; the interface offers it as a danger zone on the server's settings tab.                                                                                 |
| POST   | `/servers/{id}/action`  | `{action: "start"\|"stop"\|"restart"\|"kill"\|"clone"}`. Returns immediately; watch the events.                                                                                                                                                                                                 |
| POST   | `/servers/{id}/command` | `{command}` → `{via: "rcon"\|"stdin", output}`. Goes over RCON when the server has it set up, and over stdin otherwise; only RCON returns `output`. Requires `COMMANDS`.                                                                                                                        |
| GET    | `/servers/{id}/console` | Recent console lines: `{lines: [{seq, at, stream, text}]}`, `?after=<seq>` for the tail. `stream` is `stdout`, `stderr`, `install` for what the installer did, and `command` and `rcon` for the panel's echo of an RCON exchange.                                                              |
| DELETE | `/servers/{id}/console` | Drops the backlog the panel holds for this server, for everybody watching, and publishes `cleared` on the console topic. Sequence numbers carry on, so a client polling with `after` is unaffected. The game's own log files are untouched. Requires `CONSOLE`.                             |
| GET    | `/servers/{id}/rcon`    | `{supported, enabled, port, reachable}`. `supported` is false for Bedrock, which has no RCON. `reachable` is proven by connecting, so it is only ever true while the server is up. Requires `COMMANDS`.                                                                                         |
| POST   | `/servers/{id}/rcon`    | Turns RCON on: sets `enable-rcon`, picks a free port and writes a password into `server.properties`. `{regenerate_password}` replaces one that is already there. Returns `{port, restart_required}`. `409 CONFLICT` when the server has not written `server.properties` yet. Requires `CONFIG`. |
| GET    | `/servers/{id}/logs`    | Log files: `{files: [{name, size, modified}]}`; `?file=` returns its contents.                                                                                                                                                                                                                  |
| GET    | `/servers/stats`        | Recent history for every server the caller can see, in one call: `?minutes=&points=` → `{since, minutes, series: {"<id>": [{cpu, memory_percent} | null]}}`. Averaged into `points` buckets so a line is a fixed length; a bucket nothing was sampled in is `null`, which draws as a gap rather than a floor. |
| GET    | `/servers/{id}/players` | `{online, count, max, sampled, known, lists, operators, banned, listed, running, edition}`. `operators` and `banned` are lower-cased names, for badges; `listed` is everybody any list names. A row in `known` is only ever `online` while the server is live, so one that stops with people on it reports nobody rather than leaving them there. Requires `PLAYERS`. |
| POST   | `/servers/{id}/player-actions` | One thing to do about one player. Requires `PLAYERS`, or `COMMANDS` for `give`, `teleport`, `say` and `whisper`. |
| GET    | `/servers/{id}/items`   | What `give` will take: `{source, kind, items: [{id, name, category, icon?}]}`, sorted by id. `source` is `server` when a Bedrock add-on has listed what the running server actually holds, add-ons and all, and `catalogue` when the panel is offering the vanilla list for that edition instead. `category` is a creative tab, or `other` for anything unplaced. `icon` is present when a picture exists, and asking for the list is what starts the panel filling its picture cache. Requires `COMMANDS`. |
| GET/POST/DELETE | `/servers/{id}/players/{list}` | One of the files the game keeps beside the world. `POST {value, reason?, level?}` adds, `DELETE {value}` removes.                                                                                                                                                             |

`POST /servers/{id}/player-actions` takes one `action` and whatever that action needs:

```json
{ "action": "op",       "player": "Notch" }
{ "action": "deop",     "player": "Notch" }
{ "action": "kick",     "player": "Notch", "reason": "Back later" }
{ "action": "ban",      "player": "Notch", "reason": "Griefing" }
{ "action": "pardon",   "player": "Notch" }
{ "action": "rank",     "player": "Notch", "rank": "3" }
{ "action": "give",     "player": "Notch", "item": "diamond", "count": 8 }
{ "action": "teleport", "player": "Notch", "to": "Steve" }
{ "action": "teleport", "player": "Notch", "to": { "x": 100, "y": 64, "z": -200 } }
{ "action": "say",      "message": "Restarting in five minutes" }
{ "action": "whisper",  "player": "Notch", "message": "Nice build" }
```

The reply is `{via, ran, output?, restart_required}`.

- A running server gets the command, over RCON when it has it and stdin when it does not.
- A stopped server gets the file instead: `op`, `deop`, `ban` and `pardon` each have a list to
  write, so they work either way. The rest answer `409 CONFLICT`, because there is nobody to kick.
- `rank` is always the file. Java reads `ops.json` at start and Bedrock has no permission command,
  so a level cannot be set in game; the reply says `restart_required` while the server is up.
- `rank` is 1 to 4 on Java and `visitor`, `member` or `operator` on Bedrock.
- Bedrock has no ban command and keeps no ban list, so `ban` and `pardon` answer `409 CONFLICT`.
- Bedrock keys `permissions.json` by Xbox id, not by name. The allow list is the one file holding
  both, so the panel translates a name through it, and reads ids back out as names. A player the
  server has never seen has no id written down yet, and writing there is refused rather than
  making a row the game ignores.
- Every argument is refused if it carries a control character. A command leaves over stdin as one
  line, so a line break in a name would be a second command.


A server object is flat and honest about what is derived:

```json
{
  "id": "…",
  "name": "Survival",
  "kind": "minecraft_java",
  "created_at": "…",
  "address": { "host": "0.0.0.0", "port": 25565 },
  "settings": {
    "autostart": false,
    "crash_detection": true,
    "stop_command": "stop",
    "shutdown_timeout": 60,
    "java": {
      "binary": "java",
      "min_memory_mb": 1024,
      "max_memory_mb": 4096,
      "flags": []
    },
    "executable": "server.jar"
  },
  "state": "running",
  "stats": {
    "cpu_percent": 12.4,
    "memory_bytes": 1503238553,
    "memory_percent": 4.2,
    "players_online": 3,
    "players_max": 20,
    "version": "1.21.11",
    "motd": "A Minecraft Server",
    "started_at": "…",
    "world_size_bytes": 1048576
  },
  "flags": {
    "installing": false,
    "updating": false,
    "backing_up": false,
    "crashed": false,
    "last_backup_failed": false,
    "update_available": false
  },
  "permissions": ["COMMANDS", "CONSOLE", "…"]
}
```

`state` is one of `stopped | starting | running | stopping | crashed | installing`.

`stats.motd`, `stats.version`, `stats.latency_ms` and the player counts come from asking the
server itself: the Server List Ping on Java, RakNet's unconnected ping on Bedrock. They are null
until it answers, and stay null for a server that never does. A Bedrock server only listens for
that ping when `transport=raknet`; under Mojang's default of `nethernet` there is no port to ask.

The player lists are the JSON files the game itself keeps: `ops.json`, `whitelist.json`,
`banned-players.json` and `banned-ips.json` on Java, `allowlist.json` and `permissions.json` on
Bedrock. `GET /servers/{id}/players` names the ones that kind of server has. A file that does not
exist yet reads as an empty list, because the server writes it the first time it has something to
put there, and a write goes beside the file and renames so a crash cannot leave half of one.

Adding to a Java list resolves the name with Mojang first, since those files are keyed by UUID and
a row without one is a row the server ignores. A name nobody holds is refused. Bedrock takes the
gamertag or Xbox id as given.

`online` is what the server said when last asked. Java sends a sample rather than the whole list,
and none at all under `hide-online-players`, which `sampled` flags so the interface can say so.

## Creating a server

`POST /servers` takes the settings and one `source`. Everything but `name`, `agree_to_eula` and
`source` has a default.

```json
{
  "name": "Survival",
  "host": "0.0.0.0",
  "port": 25565,
  "min_memory_mb": 1024,
  "max_memory_mb": 4096,
  "java_binary": null,
  "java_flags": "",
  "autostart": false,
  "agree_to_eula": true,
  "properties": { "difficulty": "hard", "hardcore": true, "max-players": 40 },
  "source": { "type": "provider", "provider": "paper", "version": "1.21.11" }
}
```

The other three sources name their own `kind`, because nothing else can tell the panel whether the
files are Java or Bedrock:

```json
{ "type": "url",    "kind": "minecraft_bedrock", "url": "https://…/bedrock-server-1.26.51.1.zip", "executable": "bedrock_server" }
{ "type": "zip",    "kind": "minecraft_java", "upload_id": "…", "internal_path": "MyOldServer", "executable": "server.jar" }
{ "type": "folder", "kind": "minecraft_java", "path": "/mnt/user/appdata/minecraft", "executable": "server.jar" }
```

- `java_binary` null lets the panel choose. Where a project states a floor it supports anything
  above, such as Paper, it takes the newest installed runtime; where the project names the runtime
  a release was built for, such as Mojang's own manifest, it stays as close to that as the host
  allows, because Minecraft 1.16 asks for Java 8 and does not survive Java 25.
- `java_flags` empty means the flags the project recommends for itself, which Paper publishes and
  the rest borrow, raised to Aikar's larger regions once the heap reaches 12 GB. Anything you send
  is used verbatim instead. Both the chosen runtime and the flags are written back to the server.
- `properties` writes into `server.properties`. Only keys from the catalogue for that kind of
  server are accepted, so it cannot reach `server-port`, `server-portv6` or the RCON keys, which
  the panel owns. Values may be sent as strings, numbers or booleans. Keys left out keep whatever
  the server itself defaults to.
- `executable` is optional on an import; left out, the panel looks for `server.jar`, then the
  largest jar that is not an installer, or `bedrock_server` on Bedrock.
- A `.zip` at a `url` is unpacked; anything else is kept as the server jar.
- `folder` copies rather than moves, and is **administrator only**: `CREATE_SERVER` alone does not
  grant reading arbitrary paths on the host.
- `agree_to_eula` must be true. It writes `eula.txt` for Java servers.
- Validation answers `422` with `fields` keyed as `name`, `port`, `min_memory_mb`, `agree_to_eula`,
  `source.provider`, `source.version`, `source.kind`, `source.url`, `source.path`,
  `source.upload_id`.

The reply is `201 {"id": "…"}` as soon as the row exists. Downloading, unpacking and configuring
happen after that, reported as `install` events on `server:<id>` and as `install` console lines. A
failed install leaves the server in place with an empty start command so the console can be read.

| Method | Path                                    | Purpose                                                                                                                                                 |
| ------ | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| POST   | `/servers/import/upload`                | Streams a zip body to the panel and returns `201 {upload_id, size_bytes}`. Uploads are dropped after 24 hours or once they are imported. Max 16 GiB.   |
| GET    | `/servers/import/{upload_id}/entries`   | Folders in the archive that look like a server: `[{path, files}]`, for choosing `internal_path`. `path` is empty for the top level.                     |

## Providers

| Method | Path                             | Purpose                                                                                                        |
| ------ | -------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| GET    | `/providers`                     | `[{id, name, kind, summary, default_port, needs_java}]`. `vanilla`, `paper`, `purpur`, `fabric`, `neoforge`, `bedrock`. |
| GET    | `/providers/{provider}/versions` | `[{id, label, stable}]`, newest first. `503 UNAVAILABLE` when the upstream service cannot be reached.          |

Both require `CREATE_SERVER`. Version lists are fetched from upstream and held for ten minutes.

## Item pictures

| Method | Path                 | Purpose                                                                                                |
| ------ | -------------------- | -------------------------------------------------------------------------------------------------------- |
| GET    | `/items/{id}/icon`   | The picture of one item, as a png. `404` when the id is unknown or has no picture. Any signed-in caller. |

Not nested under a server: a diamond looks the same whoever asked. The pictures come from
[mcitemgallery.com](https://mcitemgallery.com), whose repository is MIT; the textures themselves
are Mojang's, so the panel fetches them at runtime rather than shipping them in its own image.

The panel fetches, never the browser. A panel on a home network can reach the internet when the
machine looking at it might not, and the gallery then sees one request per item rather than one
per person. The first call to `GET /servers/{id}/items` starts a one-off background fill: the
gallery's sets are incremental, so a dozen archives laid out as `<version>/<id>.png` unpack into
`config/icons/` and become the whole current set, about 16 MB. A `.done` file marks a set that
finished, so a run cut short is done again rather than trusted. Anything still missing afterwards
is fetched one at a time on request and kept.

`id` is looked up in the catalogue before it reaches the disk or the network, so what is asked
for never becomes a path or a URL. A panel with no way out answers `404` and the interface draws
the list without pictures, which is also what a Bedrock-only or add-on item gets: the gallery is
Java's, and no add-on ships a picture the panel could find.

## Server settings

| Method | Path                         | Purpose                                                                                                   |
| ------ | ---------------------------- | ----------------------------------------------------------------------------------------------------------- |
| GET    | `/properties?kind=`          | The catalogue for a kind of server, before one exists. Any signed-in caller.                              |
| GET    | `/servers/{id}/properties`   | One server's file: `{exists, settings, other}`. Requires `CONFIG`.                                        |
| PUT    | `/servers/{id}/properties`   | `{settings: {key: value}}`, merged in. Returns `{changed, restart_required}`. Requires `CONFIG`.          |

On a read, `settings` is the catalogue with each entry's current `value` and a `set` flag saying
whether the file names it at all; when `set` is false the server's own default applies. `other` is
every remaining key in the file, each with `managed` marking the ones the panel writes itself.

A write merges: keys left out are untouched. Keys in the catalogue are checked against it, keys
outside it are accepted as text so the rest of the file stays editable, and the reserved keys
(`server-port`, `server-portv6`, `enable-rcon`, `rcon.port`, `rcon.password`) are refused. A
single bad key fails the whole request before anything reaches disk.

`GET /properties?kind=minecraft_java` returns the `server.properties` keys the panel knows how to
present, for any signed-in caller. `404` for a kind it does not know.

```json
[
  { "key": "difficulty", "label": "Difficulty", "help": "", "group": "World",
    "default": "easy", "type": "choice", "options": ["peaceful", "easy", "normal", "hard"] },
  { "key": "max-players", "label": "Player slots", "help": "", "group": "Players",
    "default": "20", "type": "number", "min": 1, "max": 1000 },
  { "key": "hardcore", "label": "Hardcore", "help": "…", "group": "World",
    "default": "false", "type": "flag" }
]
```

`type` is `text`, `flag`, `number` (with `min` and `max`) or `choice` (with `options`). `group` is
a heading to lay the fields out under. Java and Bedrock name several of the same ideas
differently, so the two lists overlap only in part: `motd` against `server-name`, `white-list`
against `allow-list`. The keys that no longer exist in current Minecraft, such as `pvp` and
`allow-nether`, are not offered.

Both editions start with the allow list **on**, so a server nobody has been added to lets nobody
in. The defaults here say so rather than guessing the friendlier answer.

Experiments split by edition. Java turns them on through `initial-enabled-packs`, a
comma-separated list read only when the world is first made, so it is offered here. Bedrock keeps
its toggles in the world's `level.dat` instead, where `server.properties` cannot reach them; that
needs the world editing which is still on the roadmap.
Mojang publishes only the current Bedrock server, so that provider lists the release and the
preview; anything older has to arrive as a `url` or a `zip`.

## Files, backups, schedules, webhooks

| Method           | Path                                                     | Purpose                                                                                                                                                                                                                                                  |
| ---------------- | -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET              | `/servers/{id}/files?path=`                              | `{path, entries: [{name, kind, size, modified}]}`, directories first. Traversal outside the server folder is `403`, including through a symbolic link.                                                                                                   |
| GET              | `/servers/{id}/files/content?path=`                      | `{content, encoding, size, modified}`; `422` for binary.                                                                                                                                                                                                 |
| PUT              | `/servers/{id}/files/content`                            | `{path, content, modified}`; `409 CONFLICT` when the file changed since `modified`.                                                                                                                                                                      |
| POST             | `/servers/{id}/files`                                    | `{path, kind: "file"\|"directory"}`.                                                                                                                                                                                                                     |
| PATCH            | `/servers/{id}/files`                                    | `{path, new_name}` to rename, or `{paths, destination, mode: "copy"\|"move"}`.                                                                                                                                                                           |
| DELETE           | `/servers/{id}/files`                                    | `{paths}`.                                                                                                                                                                                                                                               |
| POST             | `/servers/{id}/files/upload`                             | Streamed upload, `?path=` for the folder. Progress is an event.                                                                                                                                                                                          |
| GET              | `/servers/{id}/files/download?path=`                     | Streams the file, or a zip of a folder.                                                                                                                                                                                                                  |
| POST             | `/servers/{id}/files/extract`                            | `{path}` to unzip in place.                                                                                                                                                                                                                              |
| GET              | `/servers/{id}/packs`                                    | `{packs: [{name, sort, path, uuid, version, activated, stock}], missing: [{uuid, version, sort}]}`. Read off the folders, so a pack dropped in by hand is listed too. Requires `FILES`. |
| DELETE           | `/servers/{id}/packs`                                    | `{path}` removes an installed pack: its files, and its id from every world that names it. `{uuid, world}` instead takes an id out of one world's list, for an entry with no pack behind it. `404` if neither is found. Requires `FILES`. |
| POST             | `/servers/{id}/packs`                                    | `{path, activate, world, use_world}` installs an add-on already in the server folder. Returns `{installed, world, level_name, restart_required}`. Requires `FILES`. |
| POST             | `/servers/{id}/packs/upload?name=&activate=&world=&use_world=` | The archive itself as the body, unpacked without ever landing in the server folder. Same reply. Requires `FILES`. |
| GET              | `/servers/{id}/packs/notes/{pack-uuid}`                  | `{text, commands: [{label, command}], suggestions: [{kind, label, command, detail}]}`. The note, and what the pack itself looks like it answers to. Requires `FILES`. |
| PUT              | `/servers/{id}/packs/notes/{pack-uuid}`                  | `{text, commands}` saves it; a note with neither is forgotten rather than stored empty. Requires `CONFIG`. |
| GET              | `/servers/{id}/worlds`                                   | `{worlds: [{name, folder, path}], level_name}`. A world is a folder holding a `level.dat`. Requires `FILES`. |
| PUT              | `/servers/{id}/worlds`                                   | `{folder}` points `level-name` at a world the server already keeps. Requires `FILES`. |
| GET/POST/DELETE  | `/servers/{id}/bridge`                                   | The Crustation add-on on a Bedrock server. `GET` needs `CONSOLE`, the rest `CONFIG`. Java answers `409 CONFLICT`: it has RCON. |
| GET/POST         | `/servers/{id}/macros`                                   | Saved commands as buttons: `{id, label, command, position}`. Reading needs `COMMANDS`, changing needs `CONFIG`.                                                                                                                                          |
| PATCH/DELETE     | `/servers/{id}/macros/{macro_id}`                        | One saved command.                                                                                                                                                                                                                                       |
| GET/POST         | `/servers/{id}/backups`                                  | Backup configs: name, destination, retention, excludes, compression, whether to stop the server.                                                                                                                                                        |
| GET/PATCH/DELETE | `/servers/{id}/backups/{backup_id}`                      | One config.                                                                                                                                                                                                                                              |
| POST             | `/servers/{id}/backups/{backup_id}/run`                  | Start a run; progress arrives as events.                                                                                                                                                                                                                 |
| GET              | `/servers/{id}/backups/{backup_id}/archives`             | `[{name, size, created_at}]`.                                                                                                                                                                                                                            |
| POST             | `/servers/{id}/backups/{backup_id}/restore`              | `{archive, wipe_first}`.                                                                                                                                                                                                                                 |
| GET              | `…/archives/{name}/download`, DELETE `…/archives/{name}` | Fetch or remove an archive.                                                                                                                                                                                                                              |
| GET/POST         | `/servers/{id}/schedules`                                | `{name, enabled, trigger, action}` where `trigger` is `{type:"interval", every_seconds, at?}`, `{type:"cron", expression}` or `{type:"after", schedule_id, delay_seconds}`, and `action` is `{type:"start"\|"stop"\|"restart"\|"backup"\|"command", …}`. |
| GET/PATCH/DELETE | `/servers/{id}/schedules/{schedule_id}`                  | One schedule. `POST …/run` runs it now.                                                                                                                                                                                                                  |
| GET/POST         | `/servers/{id}/webhooks`                                 | `{name, provider, url, events, template, enabled}`. `POST …/{webhook_id}/test` sends a test.                                                                                                                                                             |

`POST /servers/{id}/packs` reads a `.mcaddon`, `.mcpack` or plain `.zip` and puts what is inside
where the server looks for it. A `.mcaddon` may hold several packs, sometimes as `.mcpack` files
nested inside it, and both shapes are handled.

- Each pack is found by its `manifest.json`, and its first recognised module decides where it
  goes: `data`, `script` and `client_data` are behaviour packs, `resources` a resource pack.
- Behaviour packs land in `behavior_packs/<name>`, resource packs in `resource_packs/<name>`, and
  the folder is replaced rather than merged, so an update leaves nothing of the old one behind.
  Replacing only ever happens when the folder already holds **the same pack**, by id. Two packs
  that call themselves the same thing get separate folders, the second marked with its own id.
  Letting them share would mean the second deleted the first while both stayed in the world's
  list, and the server would start up saying a configured pack was not found.
- Each pack carries a `note`, or `null`: what somebody wrote down about it and the commands worth
  a button. Plenty of add-ons need something run in the game before they do anything, and the pack
  is the only place that says so. Notes are per server, keyed by the pack's id, so an update or a
  reinstall into another folder keeps one. Running a command is the ordinary `POST
  /servers/{id}/command`, so it needs `COMMANDS` and it runs as the server: one aimed at `@s` or
  `@p` still wants typing in the game.
- `suggestions` are read out of the pack itself, as starting points rather than a promise: every
  `functions/` file except the ones `tick.json` already runs, the script event ids its scripts
  compare against, the slash commands they register, and the settings its manifest declares. A
  `setting` has no `command`, since the world is where it is changed. A script that hides its own
  names behind variables is read only as far as it can be.
- `missing` is the other half of that: ids the world's lists name with no pack behind them, which
  is exactly what the server reports once at startup and then never again. `DELETE` on the same
  path takes one out of the list; the pack's own folder, if it has one, is untouched.
- `name` in a listing is for showing, not matching. The game's `§` colour codes are taken out,
  and a header that holds a key such as `pack.name` is looked up in the pack's
  `texts/en_US.lang`, or the first language it ships. The folder name stands in when neither
  says anything, and always for the stock packs, which would otherwise all read the same. The
  folder on disk is still named from the header as written, so an update finds the folder it
  replaced.
- `activate` also writes the pack into `worlds/<level-name>/world_behavior_packs.json` or
  `world_resource_packs.json`, because Bedrock ignores a pack that is only sitting in the folder.
  The list is **added to, never replaced**: the file is read, the new pack appended, and every
  pack already named there kept. Installing the same pack again updates its row in place rather
  than adding a second one.
- `stock` marks a pack the Bedrock server unpacked for itself rather than one anybody chose.
  A dedicated server ships dozens — `vanilla`, `editor`, the two script libraries, and
  `chemistry` once per game version — which bury the one pack somebody added, so the interface
  folds them away behind a switch. It is judged by the folder name, with the version suffix
  taken off, so `chemistry_1.20.50` is the 1.20.50 copy of `chemistry`. Nothing is hidden from
  the file browser and nothing is deleted; it only decides what the add-ons tab shows first.
- If that file exists but cannot be read as a list, the panel refuses to touch it rather than
  writing a fresh one over the top, which would switch off every pack the world already loads.
  The pack's own files still land, and it comes back `activated: false` so the interface can say
  the world was not told about it.
- On Java the same endpoint takes a datapack: a zip with a `pack.mcmeta`, unpacked into
  `<level-name>/datapacks/`.
- A `header.name` of `pack.something` is a key looked up in the language file the pack carries,
  not a name, so the file or folder name is used instead.
- An archive with nothing the panel recognises answers `409 CONFLICT` rather than scattering
  files about.
- It always takes effect on the next start, so the reply says `restart_required`.

Two ways in, one set of rules. `POST /servers/{id}/packs` reads a file already in the server
folder, which is how the file browser installs one. `POST /servers/{id}/packs/upload` takes the
archive as the request body, which is how the **Install add-on** and **Import world** buttons
work: the file goes to a scratch folder outside every server, is unpacked from there, and is
deleted whether or not it worked, so a half-arrived upload is never something the browser can
show. `name` carries the original filename, since the extension is what says how to read it.

- `world` names which world the packs are switched on for, rather than always the one
  `level-name` points at. It has to be a folder the server actually keeps — it is joined onto a
  path, so an unknown one answers `422` rather than being written anywhere.
- `use_world` decides whether an imported world becomes the one the server plays. It follows
  `activate` when it is not given, which is what the file browser has always done. The buttons
  send `false`, so importing a world puts it in place without changing what is running, and
  `PUT /servers/{id}/worlds` switches when you mean to.
- The reply's `world` says which world the packs were switched on for, and `level_name` is set
  only when the server was pointed at a world that just arrived.

Every file endpoint needs `FILES`. Paths are relative to the server's own folder and are checked
twice: once as text, throwing out `..` and anything anchored elsewhere, and again after resolving
the real path, because a symbolic link inside the folder can still point anywhere on the disk.

Reading returns text only, up to 2 MB; anything larger or holding a null byte answers `422` and
should be downloaded instead. A write may carry the `modified` it last saw, and gets `409` if the
file changed underneath it. Downloading a folder streams a zip built on the servers volume rather
than inside the server, so it never shows up in the operator's own listing.


## The Bedrock add-on

Bedrock has no RCON and writes no chat to its console, so the panel can see almost nothing of
what happens on one. `POST /servers/{id}/bridge` installs a behaviour pack that tells it:

- the pack goes in `behavior_packs/crustation`,
- the world is told to load it in `world_behavior_packs.json`,
- `@minecraft/server-net` and `@minecraft/server-admin` are added to
  `config/default/permissions.json`, which the game will not hand out otherwise,
- the panel address and a fresh token are written to `config/<script-uuid>/variables.json`,
- every module the pack imports, `@minecraft/server` included, is repeated in
  `config/<script-uuid>/permissions.json`, since a settings folder of its own replaces the
  default allow list rather than adding to it,
- and the Beta APIs experiment is turned on in the world, since the network module is gated on
  it. That means reading and writing `level.dat`, which is little-endian NBT.

`panel_url` is where the **game server** reaches the panel, not where your browser does. It
defaults to `public_url` if one is set, and to loopback otherwise, which is right whenever the
panel and the server share a container. The reply reports `beta_apis_turned_on` and
`world_missing`; a server that has never started has no world yet, so install again after the
first start.

`GET` reports `{installed, connected, last_seen, beta_apis, world, suggested_url}`. `connected`
means heard from in the last six seconds. `DELETE` removes the pack, takes it out of every
world's list rather than only the running one, and revokes the token.

| Method | Path                    | Purpose                                                                                      |
| ------ | ----------------------- | -------------------------------------------------------------------------------------------- |
| POST   | `/bridge/{token}`       | What the add-on itself calls. `{events, players, items?}` in, `{want_items}` out. Not nested under `/servers` and takes no session: the token is the whole credential. An unknown one answers `404`. |

```json
{
  "events": [
    { "kind": "join", "player": "ohitsjudd" },
    { "kind": "chat", "player": "ohitsjudd", "message": "hello" },
    { "kind": "leave", "player": "ZoooDuck" },
    { "kind": "death", "player": "ZoooDuck", "message": "ZoooDuck fell from a high place" },
    { "kind": "note", "message": "anything else worth a console line" }
  ],
  "players": [
    { "name": "ohitsjudd", "x": 100.5, "y": 64, "z": -200.25, "dimension": "minecraft:overworld" }
  ]
}
```

Events become console lines written the way the Java server writes the same thing, so the chat
tab, the search and the colouring all suit them already without knowing where they came from.
The channel is one-way on purpose: commands already reach a Bedrock server on its own standard
input, so there is nothing to gain from letting the panel push work into the game.

The one thing the panel asks for is the item list. Every reply carries `{want_items}`, which is
true only while the panel holds no list for that server; the add-on then puts `items`, every id
`ItemTypes.getAll()` knows, into its next check-in and the panel stops asking. It is sent this
way round rather than every second because it runs to a couple of thousand ids and only changes
when the packs or the version do. A panel that restarts has forgotten it and asks again. This is
the only way a panel can know about an item an add-on brought, so it is what `GET
/servers/{id}/items` answers with whenever it has one.
## Panel

| Method    | Path                                                           | Purpose                                                      |
| --------- | -------------------------------------------------------------- | ------------------------------------------------------------ |
| GET       | `/panel/stats`                                                 | Host CPU, memory, storage, uptime. `disks` is one row per filesystem the panel's own folders sit on, with `keeps` naming which, rather than every mount the host reports. |
| GET/PATCH | `/panel/settings`                                              | Panel configuration, grouped by section.                     |
| GET       | `/panel/audit?limit=&before=`                                  | Audit entries, newest first.                                 |
| GET       | `/panel/java`                                                  | Java runtimes found on the host.                             |
| GET/PATCH | `/panel/mcp`                                                   | The MCP endpoint: `{enabled, config_default, public_url, tools, resources}`, and `{enabled}` to switch it. Administrator only. |
| GET/POST  | `/users`, GET/PATCH/DELETE `/users/{id}`                       | User administration.                                         |
| GET/POST  | `/roles`, GET/PATCH/DELETE `/roles/{id}`                       | Roles and their per-server permissions.                      |
| GET/POST  | `/users/{id}/api-keys`, DELETE `/users/{id}/api-keys/{key_id}` | API keys. The token is shown once, on create.                |
| GET       | `/status`                                                      | Unauthenticated: servers marked public, for the status page. |

Users, roles and keys are administered under `MANAGE_USERS` and `MANAGE_ROLES`; an administrator
holds both. A role carries `global_permissions` and a list of `servers`, each naming a server and
the permissions it grants there; a person's rights are the union of the roles they hold. An
unknown permission name is refused rather than dropped.

The guards against locking yourself out are enforced by the API, not the interface: you cannot
delete, disable or demote yourself, and the last enabled administrator cannot be removed. Setting
a password or disabling an account moves `sessions_valid_from`, ending every session it had open.
An API key's token appears exactly once, in the reply that creates it.

Anyone may read their own user record and manage their own keys without `MANAGE_USERS`.

## Model Context Protocol

The panel is also an MCP server, at `/mcp`, speaking Streamable HTTP. It is the same binary and
the same data; what it adds is a shape an assistant can use, so one can read a server's console
and act on it rather than be told about it second hand.

```
claude mcp add --transport http crustation https://panel.example.com/mcp \
  --header "Authorization: Bearer <api key>"
```

- **Authentication is an API key and nothing else.** A session cookie is refused here on
  purpose: a browser sends its cookie to any origin that asks, and this endpoint is reachable
  cross-site. Without a bearer token the answer is `401` with `WWW-Authenticate: Bearer`.
- Every call runs as that key's owner and goes through the same permission checks the REST API
  uses, so an MCP client reaches exactly as far as the key already could, and no further. A
  refusal comes back as an error the model can read, naming the permission it wanted.
- The writes — `server_action`, `send_command`, `set_property`, `write_file` — are recorded in the audit log
  like any other, marked as having come over MCP.
- Sessions are not kept. Each request stands alone, which is what the `2026-07-28` revision of
  the protocol expects.
- There is a screen for it, under **Administration → Settings**, which an administrator can
  reach. It shows whether the endpoint answers, the address and the `claude mcp add` line to
  paste, and every tool beside the permission it will ask for — read from the router itself, so
  the page cannot drift from what the server actually serves. A test keeps the permission column
  honest: a tool added without saying what it costs fails the build.
- The switch on that screen takes at once and is remembered. `panel.mcp_enabled` in
  `crustation.toml` (or `CRUSTATION_MCP_ENABLED`) decides only what a panel with nothing stored
  does on a fresh start; after that the stored answer wins. While it is off the endpoint answers
  `503` and nothing reaches the tools.

| Tool              | Takes                       | Needs      |
| ----------------- | --------------------------- | ---------- |
| `list_servers`    | —                           | —          |
| `server_details`  | `server`                    | `LOGS`     |
| `server_action`   | `server`, `action`          | `COMMANDS` |
| `send_command`    | `server`, `command`         | `COMMANDS` |
| `read_console`    | `server`, `lines?`          | `CONSOLE`  |
| `list_players`    | `server`                    | `PLAYERS`  |
| `read_properties` | `server`                    | `CONFIG`   |
| `set_property`    | `server`, `key`, `value`    | `CONFIG`   |
| `list_files`      | `server`, `path?`           | `FILES`    |
| `read_file`       | `server`, `path`            | `FILES`    |
| `write_file`      | `server`, `path`, `content` | `FILES`    |
| `list_packs`      | `server`                    | `FILES`    |
| `panel_overview`  | —                           | —          |

`server` is a server's id or its name, because a name is what an assistant has to hand.
`list_packs` carries each pack's note, so a pack's setup steps come back with the pack.
`read_file` refuses anything binary or over 256 KB rather than guessing at it, and every path is
checked the same way the file API checks one. `write_file` replaces the file rather than adding to
it, makes the folders along the path, refuses a folder, and stops at the same 256 KB, so nothing
is written that cannot be read back. It has no "changed since you read it" check, so two writers
racing is the last one's answer. `set_property` runs the value past the catalogue
for that edition and refuses the keys the panel writes itself.

Resources cover the same ground for a client that would rather attach state than call a tool:
`crustation://servers` is the list, and `crustation://servers/{id}/details`,
`…/console` and `…/properties` are the per-server ones, listed for every server the key can see
and declared as templates as well.

## WebSocket

One socket at `/ws`, authenticated by the same cookie. Client frames:

```json
{"type": "subscribe",   "topics": ["servers", "server:<id>:console"]}
{"type": "unsubscribe", "topics": ["server:<id>:console"]}
{"type": "ping"}
```

Server frames are `{"type":"event","topic":…,"event":…,"data":…}`, plus `{"type":"pong"}` and
`{"type":"error","error":…,"message":…}`.

| Topic                 | Events                                                                                                                              |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `servers`             | `state` (`{server_id, state, flags}`), `stats` (`{server_id, …}`), `created`, `deleted`                                             |
| `server:<id>:console` | `line` (`{seq, at, stream, text}`), `cleared` (`{}`) — requires `CONSOLE`                                                           |
| `server:<id>`         | `state`, `stats`, `players`, `install` (`{phase, percent, message}`), `backup` (`{backup_id, phase, percent}`), `upload`, `extract` |
| `panel`               | `host_stats`, `notification` (`{level, message}`), `audit`                                                                          |

Subscriptions are per connection and can change at any time, so client-side navigation does not
need to reconnect. Every event is filtered by the subscriber's permissions at send time.

## Not in v1

Two-factor authentication and passkeys. The session and user model leaves room for them: an
`auth_methods` table and a `mfa_required` flag on roles exist from the start, unused.

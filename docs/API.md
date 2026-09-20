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
| DELETE | `/servers/{id}`         | `?delete_files=true` also removes the folder.                                                                                                                                                                                                                                                   |
| POST   | `/servers/{id}/action`  | `{action: "start"\|"stop"\|"restart"\|"kill"\|"clone"}`. Returns immediately; watch the events.                                                                                                                                                                                                 |
| POST   | `/servers/{id}/command` | `{command}` → `{via: "rcon"\|"stdin", output}`. Goes over RCON when the server has it set up, and over stdin otherwise; only RCON returns `output`. Requires `COMMANDS`.                                                                                                                        |
| GET    | `/servers/{id}/console` | Recent console lines: `{lines: [{seq, at, stream, text}]}`, `?after=<seq>` for the tail. `stream` is `stdout`, `stderr`, `install` for what the installer did, and `command` and `rcon` for the panel's echo of an RCON exchange.                                                              |
| GET    | `/servers/{id}/rcon`    | `{supported, enabled, port, reachable}`. `supported` is false for Bedrock, which has no RCON. `reachable` is proven by connecting, so it is only ever true while the server is up. Requires `COMMANDS`.                                                                                         |
| POST   | `/servers/{id}/rcon`    | Turns RCON on: sets `enable-rcon`, picks a free port and writes a password into `server.properties`. `{regenerate_password}` replaces one that is already there. Returns `{port, restart_required}`. `409 CONFLICT` when the server has not written `server.properties` yet. Requires `CONFIG`. |
| GET    | `/servers/{id}/logs`    | Log files: `{files: [{name, size, modified}]}`; `?file=` returns its contents.                                                                                                                                                                                                                  |
| GET    | `/servers/{id}/stats`   | History: `?from=&to=&resolution=` → `{points: [{at, cpu, memory_bytes, memory_percent, players}]}`.                                                                                                                                                                                             |
| GET    | `/servers/{id}/players` | `{online: [...], known: [...], banned: [...]}`.                                                                                                                                                                                                                                                 |

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

Every file endpoint needs `FILES`. Paths are relative to the server's own folder and are checked
twice: once as text, throwing out `..` and anything anchored elsewhere, and again after resolving
the real path, because a symbolic link inside the folder can still point anywhere on the disk.

Reading returns text only, up to 2 MB; anything larger or holding a null byte answers `422` and
should be downloaded instead. A write may carry the `modified` it last saw, and gets `409` if the
file changed underneath it. Downloading a folder streams a zip built on the servers volume rather
than inside the server, so it never shows up in the operator's own listing.

## Panel

| Method    | Path                                                           | Purpose                                                      |
| --------- | -------------------------------------------------------------- | ------------------------------------------------------------ |
| GET       | `/panel/stats`                                                 | Host CPU, memory, disks, uptime.                             |
| GET/PATCH | `/panel/settings`                                              | Panel configuration, grouped by section.                     |
| GET       | `/panel/audit?limit=&before=`                                  | Audit entries, newest first.                                 |
| GET       | `/panel/java`                                                  | Java runtimes found on the host.                             |
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
| `server:<id>:console` | `line` (`{seq, at, stream, text}`) — requires `CONSOLE`                                                                             |
| `server:<id>`         | `state`, `stats`, `players`, `install` (`{phase, percent, message}`), `backup` (`{backup_id, phase, percent}`), `upload`, `extract` |
| `panel`               | `host_stats`, `notification` (`{level, message}`), `audit`                                                                          |

Subscriptions are per connection and can change at any time, so client-side navigation does not
need to reconnect. Every event is filtered by the subscriber's permissions at send time.

## Not in v1

Two-factor authentication and passkeys. The session and user model leaves room for them: an
`auth_methods` table and a `mfa_required` flag on roles exist from the start, unused.

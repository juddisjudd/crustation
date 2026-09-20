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
| POST   | `/servers`              | Create from a provider download or an import. Returns `{id}`; the work continues in the background and is reported over the WebSocket.                                                                                                                                                          |
| GET    | `/servers/{id}`         | One server with its settings, stats, flags and permissions.                                                                                                                                                                                                                                     |
| PATCH  | `/servers/{id}`         | Change settings. Only fields the caller may change are accepted.                                                                                                                                                                                                                                |
| DELETE | `/servers/{id}`         | `?delete_files=true` also removes the folder.                                                                                                                                                                                                                                                   |
| POST   | `/servers/{id}/action`  | `{action: "start"\|"stop"\|"restart"\|"kill"\|"clone"}`. Returns immediately; watch the events.                                                                                                                                                                                                 |
| POST   | `/servers/{id}/command` | `{command}` → `{via: "rcon"\|"stdin", output}`. Goes over RCON when the server has it set up, and over stdin otherwise; only RCON returns `output`. Requires `COMMANDS`.                                                                                                                        |
| GET    | `/servers/{id}/console` | Recent console lines: `{lines: [{seq, at, stream, text}]}`, `?after=<seq>` for the tail. `stream` is `stdout`, `stderr`, or `command` and `rcon` for the panel's echo of an RCON exchange.                                                                                                      |
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

## Files, backups, schedules, webhooks

| Method           | Path                                                     | Purpose                                                                                                                                                                                                                                                  |
| ---------------- | -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET              | `/servers/{id}/files?path=`                              | `{path, entries: [{name, kind, size, modified, readable, writable}]}`, directories first. Traversal outside the server folder is `403`.                                                                                                                  |
| GET              | `/servers/{id}/files/content?path=`                      | `{content, encoding, size, modified}`; `422` for binary.                                                                                                                                                                                                 |
| PUT              | `/servers/{id}/files/content`                            | `{path, content, modified}`; `409 CONFLICT` when the file changed since `modified`.                                                                                                                                                                      |
| POST             | `/servers/{id}/files`                                    | `{path, kind: "file"\|"directory"}`.                                                                                                                                                                                                                     |
| PATCH            | `/servers/{id}/files`                                    | `{path, new_name}` to rename, or `{paths, destination, mode: "copy"\|"move"}`.                                                                                                                                                                           |
| DELETE           | `/servers/{id}/files`                                    | `{paths}`.                                                                                                                                                                                                                                               |
| POST             | `/servers/{id}/files/upload`                             | Streamed upload, `?path=` for the folder. Progress is an event.                                                                                                                                                                                          |
| GET              | `/servers/{id}/files/download?path=`                     | Streams the file, or a zip of a folder.                                                                                                                                                                                                                  |
| POST             | `/servers/{id}/files/extract`                            | `{path}` to unzip in place.                                                                                                                                                                                                                              |
| GET/POST         | `/servers/{id}/backups`                                  | Backup configs: name, destination, retention, excludes, compression, whether to stop the server.                                                                                                                                                         |
| GET/PATCH/DELETE | `/servers/{id}/backups/{backup_id}`                      | One config.                                                                                                                                                                                                                                              |
| POST             | `/servers/{id}/backups/{backup_id}/run`                  | Start a run; progress arrives as events.                                                                                                                                                                                                                 |
| GET              | `/servers/{id}/backups/{backup_id}/archives`             | `[{name, size, created_at}]`.                                                                                                                                                                                                                            |
| POST             | `/servers/{id}/backups/{backup_id}/restore`              | `{archive, wipe_first}`.                                                                                                                                                                                                                                 |
| GET              | `…/archives/{name}/download`, DELETE `…/archives/{name}` | Fetch or remove an archive.                                                                                                                                                                                                                              |
| GET/POST         | `/servers/{id}/schedules`                                | `{name, enabled, trigger, action}` where `trigger` is `{type:"interval", every_seconds, at?}`, `{type:"cron", expression}` or `{type:"after", schedule_id, delay_seconds}`, and `action` is `{type:"start"\|"stop"\|"restart"\|"backup"\|"command", …}`. |
| GET/PATCH/DELETE | `/servers/{id}/schedules/{schedule_id}`                  | One schedule. `POST …/run` runs it now.                                                                                                                                                                                                                  |
| GET/POST         | `/servers/{id}/webhooks`                                 | `{name, provider, url, events, template, enabled}`. `POST …/{webhook_id}/test` sends a test.                                                                                                                                                             |

## Panel

| Method    | Path                                                           | Purpose                                                      |
| --------- | -------------------------------------------------------------- | ------------------------------------------------------------ |
| GET       | `/panel/stats`                                                 | Host CPU, memory, disks, uptime.                             |
| GET/PATCH | `/panel/settings`                                              | Panel configuration, grouped by section.                     |
| GET       | `/panel/audit?limit=&before=`                                  | Audit entries, newest first.                                 |
| GET       | `/panel/java`                                                  | Java runtimes found on the host.                             |
| GET       | `/providers`                                                   | Installable server kinds.                                    |
| GET       | `/providers/{provider}/versions`                               | Versions for one provider, newest first.                     |
| GET/POST  | `/users`, GET/PATCH/DELETE `/users/{id}`                       | User administration.                                         |
| GET/POST  | `/roles`, GET/PATCH/DELETE `/roles/{id}`                       | Roles and their per-server permissions.                      |
| GET/POST  | `/users/{id}/api-keys`, DELETE `/users/{id}/api-keys/{key_id}` | API keys. The token is shown once, on create.                |
| GET       | `/status`                                                      | Unauthenticated: servers marked public, for the status page. |

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

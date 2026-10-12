# Crustation

Run your Minecraft and Rust servers from a web page. One Rust binary supervises the servers and
serves the interface, so there is one container to deploy and one SQLite file to back up.

Crab, Rust, shells. The name wrote itself.

> **Built with AI, directed by a human.** Most of the code here is written by an AI assistant
> working to decisions I make: the architecture, the API contract in [docs/API.md](docs/API.md),
> and the order of work in [docs/ROADMAP.md](docs/ROADMAP.md).
>
> This is not prompt-and-ship. Nothing merges that fails `cargo fmt`, `clippy -D warnings`, the
> test suite or the interface build, and features are exercised against a running panel before
> they go out. It runs on your hardware, so you should know how it was made.

![Overview](docs/screenshots/overview.png)

## What works today

For every server:

- **Start, stop, restart and kill**, with crash detection and restart on boot. A server that is
  still booting can be stopped too.
- **A live console** you can type into, with level filters, search, colour and saved commands.
- **A file manager**: browse, edit text in place, upload, download a folder as a zip, unpack.
- **Stats** for the host and for each server process, with history.
- **Addresses and ports**: the local and public address to hand out, and whether each port answers
  from outside, asked through your own public address.
- **Schedules**: restarts with a countdown in chat, starts, stops and commands, on a repeat, a cron
  line or after another schedule.
- **Users, roles and API keys**, with per-server permissions.
- **An MCP server** at `/mcp`, so an assistant can read a console and act on a server through the
  same permissions an API key already has. See [docs/API.md](docs/API.md).

Minecraft, Java and Bedrock:

- **Create a server** from Vanilla, Paper, Purpur, Fabric, NeoForge or Bedrock. Crustation
  downloads it, accepts the EULA, sets the port and picks a Java version that suits it. Choose
  the seed, world type, difficulty, game mode and slots while you are there.
- **Import a server** you already have, from a zip, a folder on the host, or a download link.
- **Players**: who is on and who has been, operators, kicks, bans, items and teleports, and the
  allow, operator and ban lists. A stopped server gets the change in its files instead.
- **Chat**, the message of the day with its colours, and a typed `server.properties` editor.
- **Add-ons and worlds** on their own tab: install a .mcaddon or .mcpack and it is unpacked where
  the server reads it and switched on for the world you pick; import a .mcworld and it lands under
  `worlds/` under the name it gives itself.

Rust:

- **Create a server** through SteamCMD on the release, staging or aux branch, with the map, seed,
  size, slots and the rest of the start-up settings. The install says what SteamCMD is doing at
  each step, down to how much of the download is done.
- **Players**: who is on and who has been, kicks, bans for a while or for good, owners and
  moderators, mutes, items and teleports.
- **Plugins**: Oxide or Carbon on one switch, and plugins uploaded, switched off, or installed and
  updated from uMod. Oxide comes back on its own after every Steam update.
- **Chat** as it is said, and **wipes**: the map, the blueprints or everything, with a new seed if
  you want one, by hand or on a schedule.
- **Updates**: the panel notices when Steam has a newer build, then warns players in chat, stops,
  updates and starts again, on one button or whenever Facepunch ships.
- **Server health** from the game itself: frame rate, entities, queue and network.
- **Admin**: every convar with its live value, set for now or kept for every start; every console
  command with a box to run it; and every item with its picture, given to a player or to everyone.
- **F7 reports** collected on the players page with their screenshots, and a **ban list shared** by
  every Rust server on the panel.
- Commands go over websocket RCON on the loopback, so the panel's own requests never clutter the
  console.

Not there yet: backups and webhooks.
[docs/ROADMAP.md](docs/ROADMAP.md) has the order they are coming in.

## Install

### Docker

```bash
docker run -d --name crustation --stop-timeout 120 \
  -p 8080:8080 -p 25565:25565 -p 19132:19132/udp \
  -p 28015:28015/udp -p 28017:28017/udp -p 28083:28083 \
  -v /srv/crustation/config:/config \
  -v /srv/crustation/servers:/servers \
  -v /srv/crustation/backups:/backups \
  -e PUID=1000 -e PGID=1000 \
  ghcr.io/juddisjudd/crustation:latest
```

`docker-compose.yml` does the same thing. The image carries Java 8, 11, 17, 21 and 25, and each
Minecraft server starts on whichever one its version asks for.

Stopping or updating the container stops every game server first, all at once, so each can save.
A server gets its shutdown timeout, 60 seconds unless you changed it, before it is killed. Docker
only waits 10 seconds by default, which is why the examples raise it to 120. Keep it above the
longest shutdown timeout you set.

A Rust server takes its game port and the query port two above it, both UDP: 28015 and 28017 for
the first one. The Rust+ phone app reaches it on TCP 28083, the game port plus 68. SteamCMD only exists for x86, so Rust needs an amd64 host, and a server wants
around 10 GB of disk and 8 GB of memory or more.

### Unraid

The template lives in
[juddisjudd/unraid-templates](https://github.com/juddisjudd/unraid-templates), the repository
Community Applications already indexes. Two ways to add it:

- **Docker → Add Container → Template URL**, then paste
  <https://raw.githubusercontent.com/juddisjudd/unraid-templates/main/templates/crustation.xml>
- **Community Applications → Additional Repositories**, then add that repository

It maps config, servers and backups as three shares and honours `PUID` and `PGID`, so files stay
owned by your Unraid user rather than root. Its Extra Parameters carry `--stop-timeout=120`; a
container added from an older copy of the template should have that set by hand, under Advanced
View.

### From source

```bash
cd web && pnpm install && pnpm build && cd ..
cargo run --release
```

Data lands in `./config`, `./servers` and `./backups`. For interface work, run `pnpm dev` in
`web/`; Vite proxies the API and the WebSocket to the panel on port 8080.

## First sign-in

Open <http://localhost:8080>. The panel creates an administrator the first time it starts.

Set `CRUSTATION_ADMIN_USERNAME` and `CRUSTATION_ADMIN_PASSWORD` to choose your own. Otherwise
Crustation generates a password and writes it to `config/first-login.txt`. Change it after signing
in, then delete that file.

![Creating a server](docs/screenshots/new-server.png)

![A server](docs/screenshots/server.png)

![Console](docs/screenshots/console.png)

## Configuration

`config/crustation.toml` appears on first start and can be edited. Environment variables override
it, which is how the container is configured.

| Variable | Meaning |
| --- | --- |
| `CRUSTATION_PORT`, `CRUSTATION_ADDRESS` | Where the panel listens |
| `CRUSTATION_PUBLIC_URL` | External address behind a reverse proxy; turns on secure cookies |
| `CRUSTATION_CONFIG_DIR`, `CRUSTATION_SERVERS_DIR`, `CRUSTATION_BACKUPS_DIR` | Data locations |
| `CRUSTATION_ADMIN_USERNAME`, `CRUSTATION_ADMIN_PASSWORD` | First administrator |
| `CRUSTATION_LOG` | Log filter, for example `debug` |
| `CRUSTATION_MCP_ENABLED` | Set to `false` to stop serving the MCP endpoint at `/mcp` |

## Why another one

[Crafty Controller](https://gitlab.com/crafty-controller/crafty-4) has the right idea and I ran it
for a while. Crustation is the same tool on different foundations: one binary and one SQLite file
to deploy, an API built for a single-page interface, and a WebSocket that re-checks permissions on
every event it sends.

## Contributing

```
src/            the panel: http, auth, supervisor, providers, installs, stats, events
migrations/     SQLite schema
web/            the Svelte interface
docker/         container entrypoint
docs/           architecture, API contract, roadmap
```

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) covers how the pieces fit together.
[docs/API.md](docs/API.md) is the contract between the panel and the interface; change one side
without the other and things break quietly.

Before opening a pull request:

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd web && pnpm check && pnpm lint && pnpm build
```

## Licence

AGPL-3.0-or-later. Run a modified copy as a service and you owe the world your changes.

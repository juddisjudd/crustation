# Crustation

A control panel for game servers. One Rust binary supervises the server processes and serves a
Svelte web interface. Crab, Rust, shells — the name wrote itself.

**Status: early.** Sign-in, the server list, start/stop/restart/kill, a live console you can type
into, host and per-process stats, and the Docker and Unraid packaging all work. Creating a server
from the interface does not exist yet, nor do files, backups, schedules, users or webhooks.
[docs/ROADMAP.md](docs/ROADMAP.md) tracks the order I'm building them in.

![Overview](docs/screenshots/overview.png)

![Console](docs/screenshots/console.png)

## Why another one

[Crafty Controller](https://gitlab.com/crafty-controller/crafty-4) has the right idea and I ran it
for a while. Crustation is my take on the same tool with different foundations: one binary and one
SQLite file to deploy, an API built for a single-page interface, and a WebSocket that re-checks
permissions on every event it sends.

## Running it

### Docker

```bash
docker run -d --name crustation \
  -p 8080:8080 -p 25565:25565 -p 19132:19132/udp \
  -v /srv/crustation/config:/config \
  -v /srv/crustation/servers:/servers \
  -v /srv/crustation/backups:/backups \
  -e PUID=1000 -e PGID=1000 \
  ghcr.io/juddisjudd/crustation:latest
```

`docker-compose.yml` does the same thing. The image carries Java 8, 17 and 21 for the game servers.

On first start the panel creates an administrator. Set `CRUSTATION_ADMIN_USERNAME` and
`CRUSTATION_ADMIN_PASSWORD`, or let it generate a password into `config/first-login.txt`. Then open
<http://localhost:8080>.

### Unraid

`docker/unraid.xml` is a Community Applications template. Add it through **Docker → Add Container →
Template URL**. It maps config, servers and backups as three shares and honours `PUID` / `PGID`, so
files stay owned by your Unraid user instead of root.

### From source

```bash
cd web && pnpm install && pnpm build && cd ..
cargo run --release
```

Data lands in `./config`, `./servers` and `./backups`. For interface work run `pnpm dev` in `web/`;
the Vite server proxies the API and WebSocket to the panel on port 8080.

## Configuration

`config/crustation.toml` appears on first start and can be edited. Environment variables override
it, which is how the container is configured:

| Variable | Meaning |
| --- | --- |
| `CRUSTATION_PORT`, `CRUSTATION_ADDRESS` | Where the panel listens |
| `CRUSTATION_PUBLIC_URL` | External address behind a reverse proxy; turns on secure cookies |
| `CRUSTATION_CONFIG_DIR`, `CRUSTATION_SERVERS_DIR`, `CRUSTATION_BACKUPS_DIR` | Data locations |
| `CRUSTATION_ADMIN_USERNAME`, `CRUSTATION_ADMIN_PASSWORD` | First administrator |
| `CRUSTATION_LOG` | Log filter, for example `debug` |

## Layout

```
src/            the panel: http, auth, supervisor, stats, events
migrations/     SQLite schema
web/            the Svelte interface
docker/         entrypoint and the Unraid template
docs/           architecture, API contract, roadmap
```

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) covers how the pieces fit together.
[docs/API.md](docs/API.md) is the contract between the panel and the interface; change one side
without the other and things break quietly.

## Licence

AGPL-3.0-or-later. Run a modified copy as a service and you owe the world your changes.

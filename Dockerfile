# 1. Build the web interface
FROM node:22-alpine AS web
WORKDIR /web
RUN npm install -g pnpm@11
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml web/.npmrc ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# 2. Build the panel
FROM rust:1-slim-bookworm AS panel
WORKDIR /src
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*
# Cache dependencies before the sources change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY migrations ./migrations
COPY src ./src
RUN touch src/main.rs && cargo build --release

# 3. Runtime: the panel plus every Java a Minecraft release has ever asked for.
# 8 covers up to 1.16, 11 the 1.12-1.16 modpacks, 17 up to 1.20.4, 21 the 1.21
# line, and 25 the calendar releases from 26.1 on. The install picks between them.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    LOG4J_FORMAT_MSG_NO_LOOKUPS=true \
    CRUSTATION_CONFIG_DIR=/config \
    CRUSTATION_SERVERS_DIR=/servers \
    CRUSTATION_BACKUPS_DIR=/backups \
    CRUSTATION_PORT=8080 \
    CRUSTATION_WEB_DIR=/app/web \
    CRUSTATION_ADDONS_DIR=/app/addons \
    CRUSTATION_DOCKER=1 \
    PUID=99 \
    PGID=100

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl gosu tzdata unzip \
        openjdk-8-jre-headless openjdk-11-jre-headless openjdk-17-jre-headless \
        openjdk-21-jre-headless openjdk-25-jre-headless \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --shell /usr/sbin/nologin crustation

COPY --from=panel /src/target/release/crustation /usr/local/bin/crustation
COPY --from=web /web/build /app/web
COPY addons /app/addons
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

VOLUME ["/config", "/servers", "/backups"]
EXPOSE 8080
# Minecraft Java, Bedrock and a range for extra servers
EXPOSE 25565 19132/udp 25500-25600

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
    CMD curl -fsS http://localhost:${CRUSTATION_PORT}/api/v1/health || exit 1

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["crustation"]

ARG BUILD_DATE
ARG BUILD_REF
ARG VERSION
LABEL org.opencontainers.image.title="Crustation" \
      org.opencontainers.image.description="Game server control panel in Rust, with a Svelte interface" \
      org.opencontainers.image.licenses="AGPL-3.0-or-later" \
      org.opencontainers.image.source="https://github.com/juddisjudd/crustation" \
      org.opencontainers.image.created=${BUILD_DATE} \
      org.opencontainers.image.revision=${BUILD_REF} \
      org.opencontainers.image.version=${VERSION}

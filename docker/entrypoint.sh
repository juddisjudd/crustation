#!/usr/bin/env bash
# Unraid hands ownership to a share user, so match the requested ids before dropping
# privileges. Everything the panel writes lives in the three volumes.
set -euo pipefail

PUID="${PUID:-99}"
PGID="${PGID:-100}"

if [ "$(id -u)" = "0" ]; then
    groupmod -o -g "$PGID" crustation 2>/dev/null || groupadd -o -g "$PGID" crustation
    usermod -o -u "$PUID" -g "$PGID" crustation

    for dir in "${CRUSTATION_CONFIG_DIR:-/config}" "${CRUSTATION_SERVERS_DIR:-/servers}" "${CRUSTATION_BACKUPS_DIR:-/backups}"; do
        mkdir -p "$dir"
        # Only fix the top level; a large servers folder should not be walked on every boot.
        chown "$PUID:$PGID" "$dir"
    done

    exec gosu "$PUID:$PGID" "$@"
fi

exec "$@"

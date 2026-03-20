#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
COMPOSE_FILE="compose.v2.yaml"
PIPE="/tmp/stealthnet-live.pipe"
PIDFILE="/tmp/stealthnet-live.pid"
CLIENT_A_SCRIPT="/tmp/stealthnet-client-a.sh"
CLIENT_B_SCRIPT="/tmp/stealthnet-client-b.sh"

cd "$ROOT"

if [ -f "$PIDFILE" ]; then
  kill "$(cat "$PIDFILE")" 2>/dev/null || true
  rm -f "$PIDFILE"
fi

rm -f "$PIPE" "$CLIENT_A_SCRIPT" "$CLIENT_B_SCRIPT"

docker compose -f "$COMPOSE_FILE" down --remove-orphans

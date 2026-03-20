#!/bin/sh
set -eu

CFG="$1"
TUN_IF="$2"
OVERLAY_NET="$3"
MODE="${4:-idle}"
BIND_IP="${5:-}"
PORT="${6:-8080}"

/app/stealthd --config "$CFG" &
PID=$!

for i in $(seq 1 100); do
  if ip link show "$TUN_IF" >/dev/null 2>&1; then
    ip link set "$TUN_IF" up || true
    ip route replace "$OVERLAY_NET" dev "$TUN_IF" || true
    break
  fi
  sleep 0.2
done

if [ "$MODE" = "http-server" ]; then
  mkdir -p /srv
  printf 'StealthNet V2 demo web from %s\n' "$BIND_IP" > /srv/index.html
  python3 -m http.server "$PORT" --bind "$BIND_IP" -d /srv &
fi

wait "$PID"

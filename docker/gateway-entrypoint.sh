#!/bin/sh
set -eu

CFG="$1"
TUN_IF="$2"
REMOTE_SUBNET="$3"

/app/stealthd --config "$CFG" &
PID=$!

for i in $(seq 1 100); do
  if ip link show "$TUN_IF" >/dev/null 2>&1; then
    ip link set "$TUN_IF" up
    ip route replace "$REMOTE_SUBNET" dev "$TUN_IF"
    wait "$PID"
    exit $?
  fi
  sleep 0.2
done

echo "TUN interface $TUN_IF was not created in time" >&2
kill "$PID" 2>/dev/null || true
wait "$PID" || true
exit 1

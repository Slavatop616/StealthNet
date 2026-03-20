#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
COMPOSE_FILE="compose.v2.yaml"
PIPE="/tmp/stealthnet-live.pipe"
PIDFILE="/tmp/stealthnet-live.pid"
CLIENT_A_SCRIPT="/tmp/stealthnet-client-a.sh"
CLIENT_B_SCRIPT="/tmp/stealthnet-client-b.sh"

cd "$ROOT"

mkdir -p captures
rm -f "$PIPE" "$PIDFILE" "$CLIENT_A_SCRIPT" "$CLIENT_B_SCRIPT"
mkfifo "$PIPE"

docker compose -f "$COMPOSE_FILE" up -d --no-build --pull never

# ждём шлюзы
for _ in $(seq 1 30); do
  if docker compose -f "$COMPOSE_FILE" ps --status running | grep -q gw-a &&
     docker compose -f "$COMPOSE_FILE" ps --status running | grep -q gw-b; then
    break
  fi
  sleep 1
done

# IP gw-b внутри transport
GW_B_IP="$(docker compose -f "$COMPOSE_FILE" exec -T gw-b sh -lc \
  "ip -o -4 addr show scope global | awk '{print \$4}' | cut -d/ -f1 | grep '^172\\.' | head -n1" \
  | tr -d '\r')"

# интерфейс gw-a, через который достижим gw-b
TRANSPORT_IF="$(docker compose -f "$COMPOSE_FILE" exec -T gw-a sh -lc \
  "ip route get $GW_B_IP | awk '{for(i=1;i<=NF;i++) if(\$i==\"dev\") {print \$(i+1); exit}}'" \
  | tr -d '\r')"

if [ -z "$TRANSPORT_IF" ]; then
  echo "Не удалось определить transport interface в gw-a"
  exit 1
fi

echo "Transport interface: $TRANSPORT_IF"

# live capture в pipe
(
  docker compose -f "$COMPOSE_FILE" exec -T gw-a \
    sh -lc "tcpdump -U -ni $TRANSPORT_IF 'udp port 7000 or udp port 7001' -w -" > "$PIPE"
) &
echo $! > "$PIDFILE"

# временные скрипты для Konsole
cat > "$CLIENT_A_SCRIPT" <<EOF
#!/usr/bin/env bash
cd "$ROOT"
exec docker compose -f "$COMPOSE_FILE" exec client-a bash
EOF

cat > "$CLIENT_B_SCRIPT" <<EOF
#!/usr/bin/env bash
cd "$ROOT"
exec docker compose -f "$COMPOSE_FILE" exec client-b bash
EOF

chmod +x "$CLIENT_A_SCRIPT" "$CLIENT_B_SCRIPT"

# workspace 1: Wireshark
hyprctl dispatch exec "[workspace 1 silent] wireshark -k -i $PIPE"
sleep 1

# workspace 2: ДВА ОТДЕЛЬНЫХ ОКНА Konsole
hyprctl dispatch exec "[workspace 2 silent] konsole --separate --hold -e $CLIENT_A_SCRIPT"
sleep 1
hyprctl dispatch exec "[workspace 2 silent] konsole --separate --hold -e $CLIENT_B_SCRIPT"
sleep 1

hyprctl dispatch workspace 1

cat <<EOF

Демо запущено.

Workspace 1:
  Wireshark live capture

Workspace 2:
  два отдельных окна Konsole:
    - client-a
    - client-b

Примеры:
  client-b: python3 -m http.server 8080
  client-a: ping 10.50.1.2
  client-a: curl http://10.50.1.2:8080/

EOF

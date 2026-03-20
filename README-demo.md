# StealthNet Docker demo

## Что делает стенд
- `gw-a` и `gw-b` — два шлюза StealthNet
- `client` и `chat-a` — узлы в подсети `10.10.0.0/24`
- `web` и `chat-b` — узлы в подсети `10.20.0.0/24`
- `sniffer` пишет транспортный трафик StealthNet в `./captures/stealthnet-transport.pcap`

## Подготовка
Создание ключей в `./keys` рядом с `compose.yaml`:

```bash
mkdir -p keys captures
printf '%s' 'MHrYFhY0MScnylFoN9mbonGR24UVhYwuRQCSNSS8mmg=' > keys/gw-a.key
printf '%s' 'sG09sYd9gs6niI3tVRd16hkh7I/R3W3QFVmP7MMEk0g=' > keys/gw-b.key
```

## Запуск
```bash
docker compose up -d --build
```

## Проверка control plane
```bash
docker compose exec gw-a /app/stealthctl --config /app/examples/gw-a.docker.toml routes show
docker compose exec gw-a /app/stealthctl --config /app/examples/gw-a.docker.toml ping stl:1:lab:zone-b:shard-03:gw-b
docker compose exec gw-a /app/stealthctl --config /app/examples/gw-a.docker.toml clients stl:1:lab:zone-b:shard-03:gw-b
```

## HTTP из одной подсети в другую
```bash
docker compose exec client curl -v http://10.20.0.10:8080/
```

## Текстовые сообщения
Приёмник уже запущен на `chat-b:9000`. Отправка из `chat-a`:

```bash
docker compose exec chat-a sh
nc 10.20.0.20 9000
```

То, что введёшь в `chat-a`, появится в логах `chat-b`:

```bash
docker compose logs -f chat-b
```

## Захват для Wireshark
Файл захвата пишет сервис `sniffer`:

```bash
ls -lh captures/stealthnet-transport.pcap
```

Открой файл `captures/stealthnet-transport.pcap` в Wireshark.

Полезный display filter:

```text
udp.port == 7000 || udp.port == 7001
```

На transport-сегменте должен быть виден только UDP-трафик StealthNet, без открытого HTTP или текста сообщений.

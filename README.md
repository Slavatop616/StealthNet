# StealthNet MVP

StealthNet is a minimal Rust implementation of the protocol skeleton we discussed:

- L3 overlay forwarding through a userspace gateway daemon;
- mapping `IP/prefix -> stealth address -> overlay next hop`;
- transport encryption with static X25519 identities and AEAD;
- local TUN device for inner IP packets;
- CLI utilities for `ping`, route inspection, and public-client listing.

## What is implemented

This repository contains a **compilable MVP target**:

- `stealthd` — gateway daemon
- `stealthctl` — local admin/diagnostic CLI
- static IP-to-stealth routing
- static overlay routing
- direct or relay forwarding by final stealth destination
- `PING_REQ/PING_RESP`
- `PUBLIC_CLIENTS_REQ/PUBLIC_CLIENTS_RESP`
- Linux TUN device support

## What is not implemented yet

- hierarchical root/zone/shard resolver network
- rotating descriptor tags
- dynamic route advertisement and withdrawal
- capability tokens
- cover traffic
- onion-style multihop encryption

The config format already contains room for the hierarchy fields.

## Build

You need a Linux machine with Rust installed.

```bash
cargo build --release
```

## Run

Start the daemon on each gateway:

```bash
sudo RUST_LOG=info ./target/release/stealthd --config examples/gw-a.toml
sudo RUST_LOG=info ./target/release/stealthd --config examples/gw-b.toml
```

Then from another shell on the same node:

```bash
./target/release/stealthctl --config examples/gw-a.toml routes show
./target/release/stealthctl --config examples/gw-a.toml routes lookup 10.20.0.10
./target/release/stealthctl --config examples/gw-a.toml ping stl:1:lab:zone-b:shard-03:gw-b
./target/release/stealthctl --config examples/gw-a.toml clients stl:1:lab:zone-b:shard-03:gw-b
```

## Linux notes

- `stealthd` needs permission to open `/dev/net/tun` and configure the link, so run it with `CAP_NET_ADMIN` or as root.
- The daemon shells out to `ip` to assign the TUN address and bring the interface up.
- You still need appropriate host/container routes so traffic for the remote subnet reaches the TUN-backed gateway.

## Suggested first demo

- Gateway A owns `10.10.0.0/24`
- Gateway B owns `10.20.0.0/24`
- each gateway has a direct overlay peer entry for the other
- application traffic is normal HTTP over private IPs
- on the inter-gateway segment only StealthNet UDP frames are visible

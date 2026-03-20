use crate::admin::{AdminRequest, AdminResponse, PingResult, RouteDisplay};
use crate::config::{ClientConfig, Config, PeerConfig, PublicClientEntry};
use crate::crypto::{decrypt, encrypt, Identity};
use crate::frame::{
    BootstrapReq, BootstrapResp, ClientRegister, ClientRegisterAck, DataMsg, ErrorMsg,
    KeepaliveMsg, Message, OuterFrame, PingReq, PingResp, PublicClientsReq,
    PublicClientsResp, MAGIC, VERSION,
};
use crate::ip::packet_destination;
use crate::routing::{ResolveResult, RoutingTable};
use crate::tun::TunDevice;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

#[derive(Clone)]
struct PeerRuntime {
    cfg: PeerConfig,
    addr: SocketAddr,
    key: [u8; 32],
}

pub struct GatewayDaemon {
    shared: Arc<Shared>,
}

struct Shared {
    config: Config,
    routing: RoutingTable,
    udp: Arc<UdpSocket>,
    tun: Option<TunDevice>,
    peers_by_id: HashMap<String, PeerRuntime>,
    peers_by_stealth: HashMap<String, String>,
    pending_pings: Mutex<HashMap<u64, mpsc::Sender<PingResult>>>,
    pending_clients: Mutex<HashMap<u64, mpsc::Sender<(String, Vec<PublicClientEntry>)>>>,
    registered_clients: Mutex<HashMap<String, PublicClientEntry>>,
    next_request_id: AtomicU64,
    local_stealth: String,
}

impl GatewayDaemon {
    pub fn new(mut config: Config) -> Result<Self> {
        let identity_path = Path::new(&config.crypto.identity_key_file);
        let identity = Identity::load_or_generate(identity_path)?;
        info!(public_key = %identity.public_key_base64(), "loaded local identity");

        let udp = Arc::new(UdpSocket::bind(&config.transport.listen).with_context(|| {
            format!("failed to bind UDP transport on {}", config.transport.listen)
        })?);

        let mut peers_by_id = HashMap::new();
        let mut peers_by_stealth = HashMap::new();
        for peer in &config.peers {
            let addr = peer
                .addr
                .parse::<SocketAddr>()
                .with_context(|| format!("invalid peer addr {}", peer.addr))?;
            let ctx = symmetric_context(&config.node.id, &peer.id);
            let key = identity.derive_shared_key(&peer.public_key, ctx.as_bytes())?;
            peers_by_stealth.insert(peer.stealth.clone(), peer.id.clone());
            peers_by_id.insert(
                peer.id.clone(),
                PeerRuntime {
                    cfg: peer.clone(),
                    addr,
                    key,
                },
            );
        }

        if role_is_client(&config) {
            perform_client_bootstrap(&mut config, &udp, &peers_by_id)?;
        }

        let routing = RoutingTable::from_config(&config.routing)?;
        let tun = match &config.tun {
            Some(tun_cfg) if tun_cfg.enabled => Some(TunDevice::create(
                &tun_cfg.name,
                tun_cfg.address.as_deref(),
                tun_cfg.mtu,
            )?),
            _ => None,
        };

        let shared = Arc::new(Shared {
            config: config.clone(),
            routing,
            udp,
            tun,
            peers_by_id,
            peers_by_stealth,
            pending_pings: Mutex::new(HashMap::new()),
            pending_clients: Mutex::new(HashMap::new()),
            registered_clients: Mutex::new(HashMap::new()),
            next_request_id: AtomicU64::new(1),
            local_stealth: config.node.stealth.clone(),
        });

        Ok(Self { shared })
    }

    pub fn run(&self) -> Result<()> {
        self.spawn_udp_loop();
        self.spawn_tun_loop();
        self.spawn_keepalive_loop();
        self.spawn_admin_loop()?;
        self.spawn_client_register_loop();
        info!(
            node = %self.shared.config.node.id,
            role = %self.shared.config.node.role,
            stealth = %self.shared.local_stealth,
            "daemon started"
        );
        loop {
            thread::park();
        }
    }

    fn spawn_udp_loop(&self) {
        let shared = Arc::clone(&self.shared);
        let udp = Arc::clone(&self.shared.udp);
        thread::spawn(move || {
            let mut buf = vec![0u8; 65535];
            loop {
                match udp.recv_from(&mut buf) {
                    Ok((n, src)) => {
                        let packet = &buf[..n];
                        if let Err(err) = handle_datagram(&shared, src, packet) {
                            warn!(%src, error = %err, "failed to handle datagram");
                        }
                    }
                    Err(err) => {
                        error!(error = %err, "udp recv failed");
                        thread::sleep(Duration::from_millis(200));
                    }
                }
            }
        });
    }

    fn spawn_tun_loop(&self) {
        let Some(tun) = self.shared.tun.clone() else {
            info!("TUN disabled; local IP injection disabled");
            return;
        };
        let shared = Arc::clone(&self.shared);
        thread::spawn(move || {
            let mut buf = vec![0u8; 65535];
            loop {
                match tun.read_packet(&mut buf) {
                    Ok(n) => {
                        let packet = &buf[..n];
                        if let Err(err) = handle_local_packet(&shared, packet) {
                            debug!(error = %err, "failed to handle local TUN packet");
                        }
                    }
                    Err(err) => {
                        warn!(error = %err, "tun read failed");
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });
    }

    fn spawn_keepalive_loop(&self) {
        let shared = Arc::clone(&self.shared);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(10));
            for peer_id in shared.peers_by_id.keys() {
                let msg = Message::Keepalive(KeepaliveMsg { ttl: 4 });
                if let Err(err) = send_to_peer(&shared, peer_id, &msg) {
                    warn!(peer = %peer_id, error = %err, "failed to send keepalive");
                }
            }
        });
    }

    fn spawn_admin_loop(&self) -> Result<()> {
        let socket_path = self
            .shared
            .config
            .admin
            .as_ref()
            .map(|a| a.unix_socket.clone())
            .unwrap_or_else(|| "/tmp/stealthd.sock".to_string());

        if Path::new(&socket_path).exists() {
            std::fs::remove_file(&socket_path)
                .with_context(|| format!("failed to remove existing admin socket {socket_path}"))?;
        }
        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("failed to bind admin socket {socket_path}"))?;
        let shared = Arc::clone(&self.shared);
        thread::spawn(move || {
            info!(socket = %socket_path, "admin socket listening");
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let shared = Arc::clone(&shared);
                        thread::spawn(move || {
                            if let Err(err) = handle_admin_connection(shared, stream) {
                                warn!(error = %err, "admin request failed");
                            }
                        });
                    }
                    Err(err) => warn!(error = %err, "admin accept failed"),
                }
            }
        });
        Ok(())
    }

    fn spawn_client_register_loop(&self) {
        if !role_is_client(&self.shared.config) {
            return;
        }
        let shared = Arc::clone(&self.shared);
        thread::spawn(move || loop {
            if let Err(err) = send_client_register(&shared) {
                warn!(error = %err, "client register failed");
            }
            thread::sleep(Duration::from_secs(15));
        });
    }
}

fn role_is_client(config: &Config) -> bool {
    config.node.role.eq_ignore_ascii_case("client")
}

fn role_is_gateway(config: &Config) -> bool {
    config.node.role.eq_ignore_ascii_case("gateway")
}

fn perform_client_bootstrap(
    config: &mut Config,
    udp: &UdpSocket,
    peers_by_id: &HashMap<String, PeerRuntime>,
) -> Result<()> {
    let client_cfg = config
        .client
        .clone()
        .ok_or_else(|| anyhow!("client role requires [client] config section"))?;

    let home_peer = peers_by_id
        .get(&client_cfg.home_gateway_id)
        .ok_or_else(|| anyhow!("unknown home gateway {}", client_cfg.home_gateway_id))?;

    let request_id = now_ms() as u64;
    let requested_stealth = client_cfg
        .requested_stealth
        .clone()
        .unwrap_or_else(|| config.node.stealth.clone());
    let requested_overlay_ip = client_cfg
        .requested_overlay_ip
        .clone()
        .or_else(|| config.tun.as_ref().and_then(|t| t.address.clone()));

    let msg = Message::BootstrapReq(BootstrapReq {
        request_id,
        node_id: config.node.id.clone(),
        requested_stealth: requested_stealth.clone(),
        requested_overlay_ip: requested_overlay_ip.clone(),
        capabilities: client_cfg.register_capabilities.clone(),
        target_gateway_stealth: home_peer.cfg.stealth.clone(),
    });

    send_direct(udp, &config.node.id, home_peer.addr, &home_peer.key, &msg)?;
    udp.set_read_timeout(Some(Duration::from_millis(client_cfg.bootstrap_timeout_ms)))?;

    let mut buf = vec![0u8; 65535];
    let response = loop {
        let (n, _) = udp.recv_from(&mut buf).context("bootstrap receive failed")?;
        let packet = &buf[..n];
        if let Some(resp) = decode_bootstrap_response(config, peers_by_id, packet, request_id)? {
            break resp;
        }
    };

    udp.set_read_timeout(None)?;

    if !response.ok {
        return Err(anyhow!("bootstrap rejected: {}", response.message));
    }

    config.node.stealth = response.assigned_stealth;
    if let Some(assigned) = response.assigned_overlay_ip {
        if let Some(tun) = &mut config.tun {
            if tun.address.is_none() {
                tun.address = Some(assigned.clone());
            }
        }
        if config.routing.owned_prefixes.is_empty() {
            config.routing.owned_prefixes.push(assigned);
        }
    }

    Ok(())
}

fn decode_bootstrap_response(
    config: &Config,
    peers_by_id: &HashMap<String, PeerRuntime>,
    packet: &[u8],
    expected_request_id: u64,
) -> Result<Option<BootstrapResp>> {
    let frame: OuterFrame = match bincode::deserialize(packet) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if &frame.magic != MAGIC || frame.version != VERSION {
        return Ok(None);
    }
    let peer = match peers_by_id.get(&frame.sender_id) {
        Some(value) => value,
        None => return Ok(None),
    };
    let aad = frame.sender_id.as_bytes();
    let plaintext = decrypt(&peer.key, &frame.nonce, aad, &frame.ciphertext)?;
    let msg: Message = bincode::deserialize(&plaintext)?;
    match msg {
        Message::BootstrapResp(resp) if resp.request_id == expected_request_id => Ok(Some(resp)),
        Message::Error(err) => Err(anyhow!("bootstrap error: {}", err.message)),
        _ => Ok(None),
    }
}

fn send_client_register(shared: &Shared) -> Result<()> {
    let client_cfg = shared
        .config
        .client
        .as_ref()
        .ok_or_else(|| anyhow!("missing client config"))?;
    let home_peer = shared
        .peers_by_id
        .get(&client_cfg.home_gateway_id)
        .ok_or_else(|| anyhow!("unknown home gateway {}", client_cfg.home_gateway_id))?;

    let advertised = shared
        .config
        .tun
        .as_ref()
        .and_then(|tun| tun.address.clone())
        .map(|addr| vec![addr])
        .unwrap_or_default();

    let entry = PublicClientEntry {
        public_id: client_cfg
            .public_id
            .clone()
            .unwrap_or_else(|| shared.config.node.id.clone()),
        client_stealth: shared.local_stealth.clone(),
        advertised_prefixes: advertised,
        capabilities: client_cfg.register_capabilities.clone(),
        ttl: Some(300),
    };

    let msg = Message::ClientRegister(ClientRegister {
        request_id: shared.next_request_id.fetch_add(1, Ordering::Relaxed),
        src_stealth: shared.local_stealth.clone(),
        target_gateway_stealth: home_peer.cfg.stealth.clone(),
        client: entry,
        ttl: 16,
    });

    send_to_peer(shared, &home_peer.cfg.id, &msg)
}

fn handle_local_packet(shared: &Shared, packet: &[u8]) -> Result<()> {
    let dst_ip = packet_destination(packet)
        .ok_or_else(|| anyhow!("could not parse packet destination"))?;

    if dst_ip.is_multicast() {
        debug!(dst_ip = %dst_ip, "ignoring multicast packet from TUN");
        return Ok(());
    }

    let route = shared
        .routing
        .lookup(dst_ip)
        .ok_or_else(|| anyhow!("no route for {dst_ip}"))?;

    info!(
        dst_ip = %dst_ip,
        dst_stealth = %route.stealth,
        "encapsulating local packet into DATA"
    );

    let msg = Message::Data(DataMsg {
        src_stealth: shared.local_stealth.clone(),
        dst_stealth: route.stealth.clone(),
        ttl: 16,
        inner_packet: packet.to_vec(),
    });

    forward_by_stealth(shared, &route.stealth, &msg)
}

fn handle_datagram(shared: &Shared, _src: SocketAddr, packet: &[u8]) -> Result<()> {
    let frame: OuterFrame = bincode::deserialize(packet).context("invalid outer frame")?;
    if &frame.magic != MAGIC {
        return Err(anyhow!("bad magic"));
    }
    if frame.version != VERSION {
        return Err(anyhow!("unsupported version {}", frame.version));
    }
    let peer = shared
        .peers_by_id
        .get(&frame.sender_id)
        .ok_or_else(|| anyhow!("unknown sender {}", frame.sender_id))?;
    let aad = frame.sender_id.as_bytes();
    let plaintext = decrypt(&peer.key, &frame.nonce, aad, &frame.ciphertext)?;
    let msg: Message = bincode::deserialize(&plaintext).context("invalid protocol message")?;

    match msg {
        Message::Keepalive(_) => {}
        Message::BootstrapReq(req) => {
            if role_is_gateway(&shared.config) && req.target_gateway_stealth == shared.local_stealth {
                let resp = Message::BootstrapResp(BootstrapResp {
                    request_id: req.request_id,
                    assigned_stealth: req.requested_stealth,
                    assigned_overlay_ip: req.requested_overlay_ip,
                    home_gateway_stealth: shared.local_stealth.clone(),
                    mtu: shared.config.transport.mtu,
                    ok: true,
                    message: "ok".to_string(),
                });
                send_to_peer(shared, &peer.cfg.id, &resp)?;
            }
        }
        Message::BootstrapResp(_) => {
            debug!("received asynchronous bootstrap response");
        }
        Message::ClientRegister(mut reg) => {
            if reg.ttl == 0 {
                return Ok(());
            }
            if reg.target_gateway_stealth == shared.local_stealth {
                if let Ok(mut guard) = shared.registered_clients.lock() {
                    guard.insert(reg.client.client_stealth.clone(), reg.client.clone());
                }
                let ack = Message::ClientRegisterAck(ClientRegisterAck {
                    request_id: reg.request_id,
                    src_stealth: shared.local_stealth.clone(),
                    dst_stealth: reg.src_stealth,
                    ttl: 16,
                    ok: true,
                    message: "registered".to_string(),
                });
                if let Message::ClientRegisterAck(inner) = &ack {
                    forward_by_stealth(shared, &inner.dst_stealth, &ack)?;
                }
            } else {
                reg.ttl -= 1;
                let target = reg.target_gateway_stealth.clone();
                let msg = Message::ClientRegister(reg);
                forward_by_stealth(shared, &target, &msg)?;
            }
        }
        Message::ClientRegisterAck(ack) => {
            debug!(dst = %ack.dst_stealth, ok = ack.ok, message = %ack.message, "received register ack");
        }
        Message::Data(mut data) => {
            let inner_dst = packet_destination(&data.inner_packet)
                .ok_or_else(|| anyhow!("failed to parse inner packet destination"))?;

            let is_for_this_gateway = data.dst_stealth == shared.local_stealth;
            let is_for_local_prefix = shared.routing.owns_ip(inner_dst);

            info!(
                dst_stealth = %data.dst_stealth,
                local_stealth = %shared.local_stealth,
                inner_dst = %inner_dst,
                is_for_this_gateway,
                is_for_local_prefix,
                inner_len = data.inner_packet.len(),
                "received DATA"
            );

            if is_for_this_gateway || is_for_local_prefix {
                let tun = shared
                    .tun
                    .clone()
                    .ok_or_else(|| anyhow!("TUN is not initialized"))?;
                let packet = data.inner_packet.clone();
                thread::spawn(move || {
                    info!(bytes = packet.len(), "about to write inner packet to TUN");
                    match tun.write_packet(&packet) {
                        Ok(()) => info!(bytes = packet.len(), "inner packet written to TUN"),
                        Err(err) => error!(error = %err, "failed to write inner packet to TUN"),
                    }
                });
            } else if data.ttl > 1 {
                data.ttl -= 1;
                let target = data.dst_stealth.clone();
                let msg = Message::Data(data);
                forward_by_stealth(shared, &target, &msg)?;
            } else {
                warn!(dst_stealth = %data.dst_stealth, "dropping DATA due to ttl exhaustion");
            }
        }
        Message::PingReq(mut ping) => {
            if ping.ttl == 0 {
                return Ok(());
            }
            if ping.target_stealth == shared.local_stealth {
                let resp = Message::PingResp(PingResp {
                    request_id: ping.request_id,
                    src_stealth: shared.local_stealth.clone(),
                    dst_stealth: ping.src_stealth,
                    ttl: 16,
                    timestamp_ms: ping.timestamp_ms,
                    responder_id: shared.config.node.id.clone(),
                    responder_stealth: shared.local_stealth.clone(),
                });
                if let Message::PingResp(resp_inner) = &resp {
                    forward_by_stealth(shared, &resp_inner.dst_stealth, &resp)?;
                }
            } else {
                ping.ttl -= 1;
                let target = ping.target_stealth.clone();
                let msg = Message::PingReq(ping);
                forward_by_stealth(shared, &target, &msg)?;
            }
        }
        Message::PingResp(mut pong) => {
            if pong.ttl == 0 {
                return Ok(());
            }
            if pong.dst_stealth == shared.local_stealth {
                let sender = {
                    let mut guard = shared
                        .pending_pings
                        .lock()
                        .map_err(|_| anyhow!("pending_pings lock poisoned"))?;
                    guard.remove(&pong.request_id)
                };
                if let Some(sender) = sender {
                    let result = PingResult {
                        target: pong.src_stealth.clone(),
                        ok: true,
                        rtt_ms: Some(now_ms().saturating_sub(pong.timestamp_ms)),
                        responder_stealth: Some(pong.responder_stealth.clone()),
                        responder_id: Some(pong.responder_id.clone()),
                        error: None,
                    };
                    let _ = sender.send(result);
                }
            } else {
                pong.ttl -= 1;
                let target = pong.dst_stealth.clone();
                let msg = Message::PingResp(pong);
                forward_by_stealth(shared, &target, &msg)?;
            }
        }
        Message::PublicClientsReq(mut req) => {
            if req.ttl == 0 {
                return Ok(());
            }
            if req.target_stealth == shared.local_stealth {
                let mut clients = if let Some(pc) = &shared.config.public_clients {
                    if pc.enabled {
                        pc.entries.clone()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };
                if let Ok(guard) = shared.registered_clients.lock() {
                    clients.extend(guard.values().cloned());
                }
                let resp = Message::PublicClientsResp(PublicClientsResp {
                    request_id: req.request_id,
                    src_stealth: shared.local_stealth.clone(),
                    dst_stealth: req.src_stealth,
                    ttl: 16,
                    gateway_stealth: shared.local_stealth.clone(),
                    clients,
                });
                if let Message::PublicClientsResp(inner) = &resp {
                    forward_by_stealth(shared, &inner.dst_stealth, &resp)?;
                }
            } else {
                req.ttl -= 1;
                let target = req.target_stealth.clone();
                let msg = Message::PublicClientsReq(req);
                forward_by_stealth(shared, &target, &msg)?;
            }
        }
        Message::PublicClientsResp(mut resp) => {
            if resp.ttl == 0 {
                return Ok(());
            }
            if resp.dst_stealth == shared.local_stealth {
                let sender = {
                    let mut guard = shared
                        .pending_clients
                        .lock()
                        .map_err(|_| anyhow!("pending_clients lock poisoned"))?;
                    guard.remove(&resp.request_id)
                };
                if let Some(sender) = sender {
                    let _ = sender.send((resp.gateway_stealth.clone(), resp.clients.clone()));
                }
            } else {
                resp.ttl -= 1;
                let target = resp.dst_stealth.clone();
                let msg = Message::PublicClientsResp(resp);
                forward_by_stealth(shared, &target, &msg)?;
            }
        }
        Message::Error(err) => warn!(code = err.code, message = %err.message, "received protocol error"),
    }

    Ok(())
}

fn send_to_peer(shared: &Shared, peer_id: &str, msg: &Message) -> Result<()> {
    let peer = shared
        .peers_by_id
        .get(peer_id)
        .ok_or_else(|| anyhow!("unknown peer {peer_id}"))?;
    send_direct(&shared.udp, &shared.config.node.id, peer.addr, &peer.key, msg)
}

fn send_direct(
    udp: &UdpSocket,
    sender_id: &str,
    addr: SocketAddr,
    key: &[u8; 32],
    msg: &Message,
) -> Result<()> {
    let plaintext = bincode::serialize(msg).context("failed to serialize protocol message")?;
    let aad = sender_id.as_bytes();
    let (nonce, ciphertext) = encrypt(key, aad, &plaintext)?;
    let frame = OuterFrame {
        magic: *MAGIC,
        version: VERSION,
        sender_id: sender_id.to_string(),
        nonce,
        ciphertext,
    };
    let bytes = bincode::serialize(&frame).context("failed to serialize outer frame")?;
    udp.send_to(&bytes, addr)
        .with_context(|| format!("failed to send datagram to {}", addr))?;
    Ok(())
}

fn forward_by_stealth(shared: &Shared, target_stealth: &str, msg: &Message) -> Result<()> {
    if let Some(peer_id) = shared.peers_by_stealth.get(target_stealth) {
        return send_to_peer(shared, peer_id, msg);
    }
    if let Some(via) = shared.routing.next_hop_via(target_stealth) {
        return send_to_peer(shared, via, msg);
    }
    Err(anyhow!("no overlay route for stealth destination {target_stealth}"))
}

fn handle_admin_connection(shared: Arc<Shared>, mut stream: UnixStream) -> Result<()> {
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;
    let request: AdminRequest = serde_json::from_slice(&buf).context("invalid admin request")?;
    let response = handle_admin_request(&shared, request);
    let response_bytes = serde_json::to_vec(&response).context("failed to encode admin response")?;
    stream.write_all(&response_bytes)?;
    Ok(())
}

fn handle_admin_request(shared: &Shared, request: AdminRequest) -> AdminResponse {
    match request {
        AdminRequest::RoutesShow => AdminResponse::Routes {
            routes: shared.routing.routes().iter().map(RouteDisplay::from).collect(),
            overlay_routes: shared.routing.overlay_routes(),
        },
        AdminRequest::RoutesLookup { ip } => {
            let result = IpAddr::from_str(&ip)
                .ok()
                .and_then(|addr| shared.routing.lookup_display(addr));
            AdminResponse::RouteLookup { result }
        }
        AdminRequest::Resolve { target } => {
            let result = if let Ok(ip) = IpAddr::from_str(&target) {
                shared.routing.lookup(ip).map(|route| ResolveResult {
                    stealth: route.stealth.clone(),
                    ip: Some(ip.to_string()),
                    matched_prefix: route.prefix.to_string(),
                })
            } else {
                shared.routing.reverse_lookup(&target)
            };
            AdminResponse::Resolved { target, result }
        }
        AdminRequest::Ping {
            target,
            count,
            timeout_ms,
        } => {
            let mut results = Vec::new();
            for _ in 0..count {
                let target_stealth = match resolve_target(shared, &target) {
                    Ok(value) => value,
                    Err(err) => {
                        results.push(PingResult {
                            target: target.clone(),
                            ok: false,
                            rtt_ms: None,
                            responder_stealth: None,
                            responder_id: None,
                            error: Some(err.to_string()),
                        });
                        continue;
                    }
                };

                let request_id = shared.next_request_id.fetch_add(1, Ordering::Relaxed);
                let (tx, rx) = mpsc::channel();
                if let Ok(mut guard) = shared.pending_pings.lock() {
                    guard.insert(request_id, tx);
                }
                let started = Instant::now();
                let msg = Message::PingReq(PingReq {
                    request_id,
                    src_stealth: shared.local_stealth.clone(),
                    target_stealth: target_stealth.clone(),
                    ttl: 16,
                    timestamp_ms: now_ms(),
                    optional_inner_ip: target.parse::<IpAddr>().ok().map(|ip| ip.to_string()),
                });
                let send_result = forward_by_stealth(shared, &target_stealth, &msg);
                if let Err(err) = send_result {
                    if let Ok(mut guard) = shared.pending_pings.lock() {
                        guard.remove(&request_id);
                    }
                    results.push(PingResult {
                        target: target_stealth,
                        ok: false,
                        rtt_ms: None,
                        responder_stealth: None,
                        responder_id: None,
                        error: Some(err.to_string()),
                    });
                    continue;
                }
                match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
                    Ok(mut result) => {
                        result.rtt_ms = Some(started.elapsed().as_millis());
                        results.push(result);
                    }
                    Err(_) => {
                        if let Ok(mut guard) = shared.pending_pings.lock() {
                            guard.remove(&request_id);
                        }
                        results.push(PingResult {
                            target: target_stealth,
                            ok: false,
                            rtt_ms: None,
                            responder_stealth: None,
                            responder_id: None,
                            error: Some(format!("timeout after {} ms", timeout_ms)),
                        });
                    }
                }
            }
            AdminResponse::PingResults { results }
        }
        AdminRequest::Clients { target, timeout_ms } => {
            let target_stealth = match resolve_target(shared, &target) {
                Ok(value) => value,
                Err(err) => {
                    return AdminResponse::Error {
                        message: err.to_string(),
                    }
                }
            };
            let request_id = shared.next_request_id.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = mpsc::channel();
            if let Ok(mut guard) = shared.pending_clients.lock() {
                guard.insert(request_id, tx);
            }
            let msg = Message::PublicClientsReq(PublicClientsReq {
                request_id,
                src_stealth: shared.local_stealth.clone(),
                target_stealth: target_stealth.clone(),
                ttl: 16,
            });
            if let Err(err) = forward_by_stealth(shared, &target_stealth, &msg) {
                if let Ok(mut guard) = shared.pending_clients.lock() {
                    guard.remove(&request_id);
                }
                return AdminResponse::Error {
                    message: err.to_string(),
                };
            }
            match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
                Ok((gateway_stealth, clients)) => AdminResponse::ClientsResult {
                    gateway_stealth,
                    clients,
                },
                Err(_) => {
                    if let Ok(mut guard) = shared.pending_clients.lock() {
                        guard.remove(&request_id);
                    }
                    AdminResponse::Error {
                        message: format!("timeout after {} ms", timeout_ms),
                    }
                }
            }
        }
    }
}

fn resolve_target(shared: &Shared, target: &str) -> Result<String> {
    if let Ok(ip) = IpAddr::from_str(target) {
        let route = shared
            .routing
            .lookup(ip)
            .ok_or_else(|| anyhow!("no route for {ip}"))?;
        return Ok(route.stealth.clone());
    }
    Ok(target.to_string())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
}

fn symmetric_context(a: &str, b: &str) -> String {
    if a <= b {
        format!("stealthnet:{}<->{}", a, b)
    } else {
        format!("stealthnet:{}<->{}", b, a)
    }
}

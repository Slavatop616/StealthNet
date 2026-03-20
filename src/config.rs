use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub node: NodeConfig,
    pub transport: TransportConfig,
    pub crypto: CryptoConfig,
    #[serde(default)]
    pub tun: Option<TunConfig>,
    pub routing: RoutingConfig,
    #[serde(default)]
    pub resolver: Option<ResolverConfig>,
    #[serde(default)]
    pub public_clients: Option<PublicClientsConfig>,
    #[serde(default)]
    pub admin: Option<AdminConfig>,
    #[serde(default)]
    pub client: Option<ClientConfig>,
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("failed to parse TOML {}", path.display()))?;
        Ok(cfg)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub id: String,
    pub role: String,
    pub stealth: String,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub shard: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub listen: String,
    #[serde(default)]
    pub external_addr: Option<String>,
    #[serde(default = "default_mtu")]
    pub mtu: u32,
}

fn default_mtu() -> u32 {
    1300
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoConfig {
    pub identity_key_file: String,
    #[serde(default = "default_epoch_seconds")]
    pub epoch_seconds: u64,
    #[serde(default = "default_rekey_packets")]
    pub rekey_after_packets: u64,
}

fn default_epoch_seconds() -> u64 {
    300
}

fn default_rekey_packets() -> u64 {
    100_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub name: String,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default = "default_mtu")]
    pub mtu: u32,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    #[serde(default)]
    pub owned_prefixes: Vec<String>,
    #[serde(default = "default_policy")]
    pub default_policy: String,
    #[serde(default)]
    pub static_map: Vec<StaticRouteConfig>,
    #[serde(default)]
    pub overlay_routes: Vec<OverlayRouteConfig>,
}

fn default_policy() -> String {
    "drop".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticRouteConfig {
    pub prefix: String,
    pub stealth: String,
    #[serde(default = "default_metric")]
    pub metric: u32,
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_metric() -> u32 {
    10
}

fn default_mode() -> String {
    "subnet".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayRouteConfig {
    pub destination: String,
    pub via: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverConfig {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub root_servers: Vec<String>,
    #[serde(default)]
    pub zone_servers: Vec<String>,
    #[serde(default)]
    pub cache_ttl: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicClientsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub publish_policy: Option<String>,
    #[serde(default)]
    pub max_entries: Option<usize>,
    #[serde(default)]
    pub entries: Vec<PublicClientEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicClientEntry {
    pub public_id: String,
    pub client_stealth: String,
    #[serde(default)]
    pub advertised_prefixes: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub ttl: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    #[serde(default = "default_admin_socket")]
    pub unix_socket: String,
}

fn default_admin_socket() -> String {
    "/tmp/stealthd.sock".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub home_gateway_id: String,
    #[serde(default)]
    pub public_id: Option<String>,
    #[serde(default)]
    pub requested_overlay_ip: Option<String>,
    #[serde(default)]
    pub requested_stealth: Option<String>,
    #[serde(default)]
    pub register_capabilities: Vec<String>,
    #[serde(default = "default_bootstrap_timeout_ms")]
    pub bootstrap_timeout_ms: u64,
}

fn default_bootstrap_timeout_ms() -> u64 {
    3000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub id: String,
    pub stealth: String,
    pub addr: String,
    pub public_key: String,
}

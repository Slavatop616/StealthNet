use crate::config::PublicClientEntry;
use serde::{Deserialize, Serialize};

pub const MAGIC: &[u8; 4] = b"STN1";
pub const VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OuterFrame {
    pub magic: [u8; 4],
    pub version: u8,
    pub sender_id: String,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Keepalive(KeepaliveMsg),
    BootstrapReq(BootstrapReq),
    BootstrapResp(BootstrapResp),
    ClientRegister(ClientRegister),
    ClientRegisterAck(ClientRegisterAck),
    Data(DataMsg),
    PingReq(PingReq),
    PingResp(PingResp),
    PublicClientsReq(PublicClientsReq),
    PublicClientsResp(PublicClientsResp),
    Error(ErrorMsg),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepaliveMsg {
    pub ttl: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapReq {
    pub request_id: u64,
    pub node_id: String,
    pub requested_stealth: String,
    pub requested_overlay_ip: Option<String>,
    pub capabilities: Vec<String>,
    pub target_gateway_stealth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResp {
    pub request_id: u64,
    pub assigned_stealth: String,
    pub assigned_overlay_ip: Option<String>,
    pub home_gateway_stealth: String,
    pub mtu: u32,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegister {
    pub request_id: u64,
    pub src_stealth: String,
    pub target_gateway_stealth: String,
    pub client: PublicClientEntry,
    pub ttl: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegisterAck {
    pub request_id: u64,
    pub src_stealth: String,
    pub dst_stealth: String,
    pub ttl: u8,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMsg {
    pub src_stealth: String,
    pub dst_stealth: String,
    pub ttl: u8,
    pub inner_packet: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingReq {
    pub request_id: u64,
    pub src_stealth: String,
    pub target_stealth: String,
    pub ttl: u8,
    pub timestamp_ms: u128,
    pub optional_inner_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResp {
    pub request_id: u64,
    pub src_stealth: String,
    pub dst_stealth: String,
    pub ttl: u8,
    pub timestamp_ms: u128,
    pub responder_id: String,
    pub responder_stealth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicClientsReq {
    pub request_id: u64,
    pub src_stealth: String,
    pub target_stealth: String,
    pub ttl: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicClientsResp {
    pub request_id: u64,
    pub src_stealth: String,
    pub dst_stealth: String,
    pub ttl: u8,
    pub gateway_stealth: String,
    pub clients: Vec<PublicClientEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMsg {
    pub code: u16,
    pub message: String,
}

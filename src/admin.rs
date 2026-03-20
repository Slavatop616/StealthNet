use crate::config::PublicClientEntry;
use crate::routing::{LookupResult, OverlayRouteEntry, ResolveResult, RouteEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdminRequest {
    Ping {
        target: String,
        count: u32,
        timeout_ms: u64,
    },
    Clients {
        target: String,
        timeout_ms: u64,
    },
    RoutesShow,
    RoutesLookup {
        ip: String,
    },
    Resolve {
        target: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdminResponse {
    PingResults { results: Vec<PingResult> },
    ClientsResult {
        gateway_stealth: String,
        clients: Vec<PublicClientEntry>,
    },
    Routes {
        routes: Vec<RouteDisplay>,
        overlay_routes: Vec<OverlayRouteEntry>,
    },
    RouteLookup {
        result: Option<LookupResult>,
    },
    Resolved {
        target: String,
        result: Option<ResolveResult>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub target: String,
    pub ok: bool,
    pub rtt_ms: Option<u128>,
    pub responder_stealth: Option<String>,
    pub responder_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDisplay {
    pub prefix: String,
    pub stealth: String,
    pub metric: u32,
    pub mode: String,
}

impl From<&RouteEntry> for RouteDisplay {
    fn from(value: &RouteEntry) -> Self {
        Self {
            prefix: value.prefix.to_string(),
            stealth: value.stealth.clone(),
            metric: value.metric,
            mode: value.mode.clone(),
        }
    }
}

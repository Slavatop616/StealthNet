use crate::config::{RoutingConfig, StaticRouteConfig};
use anyhow::{Context, Result};
use ipnet::IpNet;
use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct RouteEntry {
    pub prefix: IpNet,
    pub stealth: String,
    pub metric: u32,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct OverlayRouteEntry {
    pub destination: String,
    pub via: String,
}

#[derive(Debug, Clone)]
pub struct RoutingTable {
    routes: Vec<RouteEntry>,
    overlay_routes: HashMap<String, String>,
    owned_prefixes: Vec<IpNet>,
}

impl RoutingTable {
    pub fn from_config(cfg: &RoutingConfig) -> Result<Self> {
        let mut routes = Vec::new();
        for route in &cfg.static_map {
            routes.push(parse_route(route)?);
        }
        routes.sort_by_key(|r| std::cmp::Reverse(r.prefix.prefix_len()));

        let mut overlay_routes = HashMap::new();
        for entry in &cfg.overlay_routes {
            overlay_routes.insert(entry.destination.clone(), entry.via.clone());
        }

        let owned_prefixes = cfg
            .owned_prefixes
            .iter()
            .map(|p| p.parse::<IpNet>().with_context(|| format!("invalid owned prefix {p}")))
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            routes,
            overlay_routes,
            owned_prefixes,
        })
    }

    pub fn lookup(&self, ip: IpAddr) -> Option<&RouteEntry> {
        self.routes.iter().find(|r| r.prefix.contains(&ip))
    }

    pub fn lookup_display(&self, ip: IpAddr) -> Option<LookupResult> {
        self.lookup(ip).map(|route| LookupResult {
            ip: ip.to_string(),
            matched_prefix: route.prefix.to_string(),
            stealth: route.stealth.clone(),
            metric: route.metric,
            mode: route.mode.clone(),
        })
    }

    pub fn reverse_lookup(&self, stealth: &str) -> Option<ResolveResult> {
        self.routes.iter().find(|r| r.stealth == stealth).map(|route| {
            let ip = match route.prefix {
                IpNet::V4(net) => Some(net.addr().to_string()),
                IpNet::V6(net) => Some(net.addr().to_string()),
            };
            ResolveResult {
                stealth: route.stealth.clone(),
                ip,
                matched_prefix: route.prefix.to_string(),
            }
        })
    }

    pub fn routes(&self) -> &[RouteEntry] {
        &self.routes
    }

    pub fn owned_prefixes(&self) -> &[IpNet] {
        &self.owned_prefixes
    }

    pub fn owns_ip(&self, ip: IpAddr) -> bool {
        self.owned_prefixes.iter().any(|p| p.contains(&ip))
    }

    pub fn next_hop_via(&self, destination_stealth: &str) -> Option<&str> {
        self.overlay_routes.get(destination_stealth).map(String::as_str)
    }

    pub fn overlay_routes(&self) -> Vec<OverlayRouteEntry> {
        self.overlay_routes
            .iter()
            .map(|(destination, via)| OverlayRouteEntry {
                destination: destination.clone(),
                via: via.clone(),
            })
            .collect()
    }
}

fn parse_route(route: &StaticRouteConfig) -> Result<RouteEntry> {
    let prefix = route
        .prefix
        .parse::<IpNet>()
        .with_context(|| format!("invalid route prefix {}", route.prefix))?;
    Ok(RouteEntry {
        prefix,
        stealth: route.stealth.clone(),
        metric: route.metric,
        mode: route.mode.clone(),
    })
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LookupResult {
    pub ip: String,
    pub matched_prefix: String,
    pub stealth: String,
    pub metric: u32,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ResolveResult {
    pub stealth: String,
    pub ip: Option<String>,
    pub matched_prefix: String,
}

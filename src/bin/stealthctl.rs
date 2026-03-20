use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use stealthnet::admin::{AdminRequest, AdminResponse};
use stealthnet::config::Config;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long)]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Ping {
        target: String,
        #[arg(short = 'c', long, default_value_t = 4)]
        count: u32,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
    },
    Clients {
        target: String,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
    },
    Routes {
        #[command(subcommand)]
        sub: RouteCommand,
    },
    Resolve {
        target: String,
    },
}

#[derive(Debug, Subcommand)]
enum RouteCommand {
    Show,
    Lookup { ip: String },
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::load(&args.config)?;
    let socket = config
        .admin
        .as_ref()
        .map(|a| a.unix_socket.clone())
        .unwrap_or_else(|| "/tmp/stealthd.sock".to_string());

    let request = match args.command {
        Command::Ping {
            target,
            count,
            timeout_ms,
        } => AdminRequest::Ping {
            target,
            count,
            timeout_ms,
        },
        Command::Clients { target, timeout_ms } => AdminRequest::Clients { target, timeout_ms },
        Command::Routes { sub } => match sub {
            RouteCommand::Show => AdminRequest::RoutesShow,
            RouteCommand::Lookup { ip } => AdminRequest::RoutesLookup { ip },
        },
        Command::Resolve { target } => AdminRequest::Resolve { target },
    };

    let response = send_request(&socket, &request)?;
    print_response(response)
}

fn send_request(socket: &str, request: &AdminRequest) -> Result<AdminResponse> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("failed to connect to admin socket {}", socket))?;
    let data = serde_json::to_vec(request)?;
    stream.write_all(&data)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;
    let response: AdminResponse = serde_json::from_slice(&buf)?;
    Ok(response)
}

fn print_response(response: AdminResponse) -> Result<()> {
    match response {
        AdminResponse::PingResults { results } => {
            for item in results {
                if item.ok {
                    println!(
                        "reply from {} id={} time={} ms",
                        item.responder_stealth.unwrap_or_else(|| "unknown".to_string()),
                        item.responder_id.unwrap_or_else(|| "unknown".to_string()),
                        item.rtt_ms.unwrap_or(0),
                    );
                } else {
                    println!(
                        "ping {} failed: {}",
                        item.target,
                        item.error.unwrap_or_else(|| "unknown error".to_string())
                    );
                }
            }
        }
        AdminResponse::ClientsResult {
            gateway_stealth,
            clients,
        } => {
            println!("gateway: {}", gateway_stealth);
            if clients.is_empty() {
                println!("no public clients published");
            } else {
                for client in clients {
                    println!("- {} ({})", client.public_id, client.client_stealth);
                    if !client.advertised_prefixes.is_empty() {
                        println!("  prefixes: {}", client.advertised_prefixes.join(", "));
                    }
                    if !client.capabilities.is_empty() {
                        println!("  capabilities: {}", client.capabilities.join(", "));
                    }
                }
            }
        }
        AdminResponse::Routes {
            routes,
            overlay_routes,
        } => {
            println!("IP -> stealth routes:");
            for route in routes {
                println!(
                    "- {:20} -> {:40} metric={} mode={}",
                    route.prefix, route.stealth, route.metric, route.mode
                );
            }
            println!();
            println!("overlay routes:");
            for route in overlay_routes {
                println!("- {} via {}", route.destination, route.via);
            }
        }
        AdminResponse::RouteLookup { result } => {
            if let Some(result) = result {
                println!(
                    "{} matches {} -> {} metric={} mode={}",
                    result.ip, result.matched_prefix, result.stealth, result.metric, result.mode
                );
            } else {
                println!("no route");
            }
        }
        AdminResponse::Resolved { target, result } => {
            if let Some(result) = result {
                println!("target: {}", target);
                println!("stealth: {}", result.stealth);
                if let Some(ip) = result.ip {
                    println!("ip: {}", ip);
                }
                println!("matched_prefix: {}", result.matched_prefix);
            } else {
                println!("no resolution for {}", target);
            }
        }
        AdminResponse::Error { message } => return Err(anyhow!(message)),
    }
    Ok(())
}

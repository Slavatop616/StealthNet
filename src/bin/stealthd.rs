use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use stealthnet::config::Config;
use stealthnet::daemon::GatewayDaemon;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long)]
    config: PathBuf,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let config = Config::load(&args.config)?;
    let daemon = GatewayDaemon::new(config)?;
    daemon.run()
}

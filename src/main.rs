use anyhow::{anyhow, Result};
use clap::Parser;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};

use thru::config::Config;
use thru::tunnel::{Tunnel, TunnelEndpoint};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    tunnel: String,
    #[arg(short = 'c', long)]
    tls_cert_path: Option<String>,
    #[arg(short = 'k', long)]
    tls_key_path: Option<String>,
}

async fn parse_tunnel_arg(arg: &str) -> Result<(TunnelEndpoint, TunnelEndpoint)> {
    let (from, to) = arg
        .split_once("==")
        .ok_or(anyhow!("invalid tunnel: {}", arg))?;
    let from = TunnelEndpoint::try_from(from)?;
    let to = TunnelEndpoint::try_from(to)?;

    Ok((from, to))
}

async fn shutdown_signal() {
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();

    select! {
        _ = sigint.recv() => {},
        _ = sigterm.recv() => {}
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::new(args.tls_cert_path, args.tls_key_path);

    let (from, to) = parse_tunnel_arg(&args.tunnel).await?;
    let tunnel = Tunnel::open(from, to, config).await?;

    shutdown_signal().await;

    tunnel.close().await;

    Ok(())
}

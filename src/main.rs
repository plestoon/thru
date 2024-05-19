use anyhow::{anyhow, Result};
use clap::Parser;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};

use thru::config::Config;
use thru::tunnel::{Tunnel, TunnelEndpoint};

#[derive(Parser, Debug)]
#[command(author, version, about("https://github.com/plestoon/thru"), long_about = None)]
struct Args {
    /// e.g. tcp://127.0.0.1:4242==quic://example.com:4242
    #[arg(short, long)]
    tunnel: String,
    /// TLS certificate for QUIC server
    #[arg(short = 'c', long)]
    cert: Option<String>,
    /// TLS private key for QUIC server
    #[arg(short = 'k', long)]
    key: Option<String>,
    /// Server root certificate for QUIC client.
    /// It's only needed for self-signed certificates
    /// and if it hasn't been installed on the system keystore.
    #[arg(short = 'p', long)]
    peer_cert: Option<String>,
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
    rustls::crypto::ring::default_provider().install_default().unwrap();

    let args = Args::parse();
    let config = Config::new(args.cert, args.key, args.peer_cert);

    let (from, to) = parse_tunnel_arg(&args.tunnel).await?;
    let tunnel = Tunnel::open(from, to, config).await?;

    shutdown_signal().await;

    tunnel.close().await;

    Ok(())
}

use anyhow::{anyhow, Result};
use clap::Parser;
use tokio::signal::ctrl_c;

use thru::config::Config;
use thru::transport::TransportServer;
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

async fn parse_tunnel(arg: &str) -> Result<Tunnel> {
    let (from, to) = arg
        .split_once("==")
        .ok_or(anyhow!("invalid tunnel: {}", arg))?;
    let from = TunnelEndpoint::try_from(from)?;
    let to = TunnelEndpoint::try_from(to)?;
    let tunnel = Tunnel::new(from, to);

    Ok(tunnel)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::new(args.tls_cert_path, args.tls_key_path);
    let tunnel = parse_tunnel(&args.tunnel).await?;
    let server = TransportServer::start(&tunnel, &config).await?;

    ctrl_c().await.unwrap();

    server.stop().await;

    Ok(())
}
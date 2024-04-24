use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use quinn::{
    ClientConfig, Connection, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig,
    VarInt,
};
use tokio::io::copy_bidirectional;
use tokio::net::lookup_host;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::Config;
use crate::transport::TransportClient;
use crate::tunnel::Tunnel;
use crate::util::async_read_write::AsyncReadWrite;

pub type QuicStream = AsyncReadWrite<RecvStream, SendStream>;

#[derive(Debug)]
pub struct QuicServer {
    stop_token: CancellationToken,
    task_tracker: TaskTracker,
}

impl QuicServer {
    pub async fn start(tunnel: &Tunnel, app_config: &Config) -> Result<Self> {
        let mut reader = BufReader::new(File::open(
            app_config
                .tls_cert_path
                .clone()
                .ok_or(anyhow!("tls cert must be provided"))?,
        )?);
        let certs = rustls_pemfile::certs(&mut reader)?
            .into_iter()
            .map(rustls::Certificate)
            .collect();
        let mut reader = BufReader::new(File::open(
            app_config
                .tls_key_path
                .clone()
                .ok_or(anyhow!("tls key must be provided"))?,
        )?);
        let mut keys = rustls_pemfile::pkcs8_private_keys(&mut reader)?;
        let key = rustls::PrivateKey(keys.remove(0));
        let mut config = ServerConfig::with_single_cert(certs, key)?;
        let mut transport_config = TransportConfig::default();
        transport_config.max_idle_timeout(Some(app_config.quic_max_idle_timeout.try_into()?));
        transport_config.keep_alive_interval(Some(app_config.quic_keep_alive_interval));
        config.transport_config(Arc::new(transport_config));
        let endpoint = Endpoint::server(config, tunnel.from.addr.parse::<SocketAddr>().unwrap())?;
        let client = TransportClient::new(&tunnel.to, &app_config).await?;

        let stop_token = CancellationToken::new();
        let task_tracker = TaskTracker::new();
        {
            let stop_token = stop_token.clone();
            let task_tracker = task_tracker.clone();
            task_tracker.spawn(Self::run(stop_token, endpoint, client));
        }

        Ok(Self {
            stop_token,
            task_tracker,
        })
    }

    pub async fn run(
        stop_token: CancellationToken,
        endpoint: Endpoint,
        client: TransportClient,
    ) -> Result<()> {
        loop {
            select! {
                _ = stop_token.cancelled() => {
                    break;
                }
                conn = endpoint.accept() => {
                    let conn = conn.ok_or(anyhow!("not accepting connections"))?.await?;
                    let client = client.clone();
                    tokio::spawn(async move {
                        let _ = Self::handle_connection(conn, client).await;
                    });
                }
            }
        }

        endpoint.close(VarInt::from(255u8), b"server stopped");
        endpoint.wait_idle().await;

        Ok(())
    }

    pub async fn handle_connection(connection: Connection, client: TransportClient) -> Result<()> {
        loop {
            let (send, recv) = connection.accept_bi().await?;
            let stream = QuicStream::new(recv, send);
            let client = client.clone();
            tokio::spawn(async move {
                let _ = Self::handle_stream(stream, client).await;
            });
        }
    }

    pub async fn handle_stream(mut stream: QuicStream, client: TransportClient) -> Result<()> {
        let mut remote_stream = client.connect().await?;
        copy_bidirectional(&mut stream, &mut remote_stream).await?;

        Ok(())
    }

    pub async fn stop(&self) {
        self.stop_token.cancel();
        self.task_tracker.close();
        self.task_tracker.wait().await;
    }
}

#[derive(Debug, Clone)]
pub struct QuicClient {
    connection: Connection,
}

impl QuicClient {
    pub async fn new(remote_addr: &str, app_config: &Config) -> Result<Self> {
        let (host, _) = remote_addr.split_once(':').unwrap();
        let addr = lookup_host(remote_addr)
            .await?
            .find(|addr| addr.is_ipv4())
            .unwrap();
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse::<SocketAddr>().unwrap())?;
        let mut config = ClientConfig::with_native_roots();
        let mut transport_config = TransportConfig::default();
        transport_config.max_idle_timeout(Some(app_config.quic_max_idle_timeout.try_into()?));
        transport_config.keep_alive_interval(Some(app_config.quic_keep_alive_interval));
        config.transport_config(Arc::new(transport_config));
        endpoint.set_default_client_config(config);
        let connection = endpoint.connect(addr, host)?.await?;

        {
            let connection = connection.clone();
            tokio::spawn(async move {
                let error = connection.closed().await;
                println!("{}", error)
            });
        }

        Ok(Self { connection })
    }

    pub async fn connect(&self) -> Result<QuicStream> {
        let (send, recv) = self.connection.open_bi().await?;

        Ok(QuicStream::new(recv, send))
    }
}

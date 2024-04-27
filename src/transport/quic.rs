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
use tokio::sync::Mutex;
use tokio::time::sleep;
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
        transport_config.stream_receive_window(
            VarInt::from_u64(app_config.quic_stream_receive_window).unwrap(),
        );
        transport_config.send_window(app_config.quic_send_window);
        config.transport_config(Arc::new(transport_config));
        let endpoint = Endpoint::server(config, tunnel.from.addr.parse::<SocketAddr>().unwrap())?;
        let client = TransportClient::new(&tunnel.to, &app_config).await?;

        let stop_token = CancellationToken::new();
        let task_tracker = TaskTracker::new();
        {
            let stop_token = stop_token.clone();
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
                    tokio::spawn(Self::handle_connection(conn, client));
                }
            }
        }

        endpoint.close(VarInt::from(255u8), b"server stopped");
        endpoint.wait_idle().await;

        client.disconnect().await;

        Ok(())
    }

    pub async fn handle_connection(connection: Connection, client: TransportClient) -> Result<()> {
        loop {
            let (send, recv) = connection.accept_bi().await?;
            let stream = QuicStream::new(recv, send);
            let client = client.clone();
            tokio::spawn(Self::handle_stream(stream, client));
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
    endpoint: Endpoint,
    connection: Arc<Mutex<Option<Connection>>>,
}

impl QuicClient {
    pub async fn new(remote_addr: &str, app_config: &Config) -> Result<Self> {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse::<SocketAddr>().unwrap())?;
        let mut config = ClientConfig::with_native_roots();
        let mut transport_config = TransportConfig::default();
        transport_config.max_idle_timeout(Some(app_config.quic_max_idle_timeout.try_into()?));
        transport_config.keep_alive_interval(Some(app_config.quic_keep_alive_interval));
        transport_config.stream_receive_window(
            VarInt::from_u64(app_config.quic_stream_receive_window).unwrap(),
        );
        transport_config.send_window(app_config.quic_send_window);
        config.transport_config(Arc::new(transport_config));
        endpoint.set_default_client_config(config);

        let connection = Arc::new(Mutex::new(None));

        {
            let connection = connection.clone();
            let app_config = app_config.clone();
            let endpoint = endpoint.clone();
            let remote_addr = remote_addr.to_owned();

            tokio::spawn(Self::monitor_connection(
                connection,
                app_config,
                endpoint,
                remote_addr,
            ));
        }

        Ok(Self {
            endpoint,
            connection,
        })
    }

    async fn new_connection(endpoint: &Endpoint, remote_addr: &str) -> Result<Connection> {
        let (host, _) = remote_addr.split_once(':').unwrap();
        let remote_addr = lookup_host(remote_addr)
            .await?
            .find(|addr| addr.is_ipv4())
            .unwrap();
        let connection = endpoint.connect(remote_addr, host)?.await?;

        Ok(connection)
    }

    async fn recover(app_config: &Config, endpoint: &Endpoint, remote_addr: &str) -> Connection {
        loop {
            let connection = Self::new_connection(endpoint, remote_addr).await.ok();

            if let Some(conn) = connection {
                break conn;
            }

            sleep(app_config.quic_retry_interval).await;
        }
    }

    async fn monitor_connection(
        connection_lock: Arc<Mutex<Option<Connection>>>,
        app_config: Config,
        endpoint: Endpoint,
        remote_addr: String,
    ) {
        loop {
            let connection = {
                let connection = connection_lock.lock().await;
                connection.clone()
            };

            let new_connection = match connection {
                Some(conn) => {
                    conn.closed().await;
                    sleep(app_config.quic_retry_interval).await;
                    Self::recover(&app_config, &endpoint, &remote_addr).await
                }
                None => Self::recover(&app_config, &endpoint, &remote_addr).await,
            };

            let mut connection = connection_lock.lock().await;
            *connection = Some(new_connection);
        }
    }

    pub async fn connect(&self) -> Result<QuicStream> {
        let connection = {
            let connection = self.connection.lock().await;
            connection.clone()
        };

        let connection = connection.ok_or(anyhow!("connection not established"))?;
        let (send, recv) = connection.open_bi().await?;

        Ok(QuicStream::new(recv, send))
    }

    pub async fn disconnect(&self) {
        self.endpoint
            .close(VarInt::from(255u8), b"client disconnected");

        // This takes a little bit longer while retrying.
        // https://github.com/quinn-rs/quinn/issues/1102
        self.endpoint.wait_idle().await;
    }
}

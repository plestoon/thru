use crate::config::Config;
use anyhow::Result;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::transport::TransportClient;
use crate::tunnel::Tunnel;

#[derive(Debug)]
pub struct TcpServer {
    stop_token: CancellationToken,
    task_tracker: TaskTracker,
}

impl TcpServer {
    pub async fn start(tunnel: &Tunnel, config: &Config) -> Result<TcpServer> {
        let stop_token = CancellationToken::new();
        let task_tracker = TaskTracker::new();
        let listener = TcpListener::bind(&tunnel.from.addr).await?;
        let client = TransportClient::new(&tunnel.to, &config).await?;

        {
            let stop_token = stop_token.clone();
            task_tracker.spawn(Self::run(stop_token, listener, client));
        }

        Ok(TcpServer {
            stop_token,
            task_tracker,
        })
    }

    async fn run(
        stop_token: CancellationToken,
        listener: TcpListener,
        client: TransportClient,
    ) -> Result<()> {
        loop {
            select! {
                _ = stop_token.cancelled() => {
                    break;
                }
                result = listener.accept() => {
                    let (stream, _) = result?;
                    let client = client.clone();
                    tokio::spawn(Self::handle_stream(stream, client));
                }
            }
        }

        client.disconnect().await;

        Ok(())
    }

    async fn handle_stream(mut stream: TcpStream, client: TransportClient) -> Result<()> {
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

#[derive(Clone, Debug)]
pub struct TcpClient {
    remote_addr: String,
}

impl TcpClient {
    pub fn new(remote_addr: &str) -> TcpClient {
        TcpClient {
            remote_addr: remote_addr.to_string(),
        }
    }

    pub async fn connect(&self) -> Result<TcpStream> {
        Ok(TcpStream::connect(&self.remote_addr).await?)
    }
}

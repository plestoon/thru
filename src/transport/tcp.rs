use anyhow::Result;
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::tunnel::CopyStream;

#[derive(Debug)]
pub struct TcpServer {
    stop_token: CancellationToken,
    task_tracker: TaskTracker,
}

impl TcpServer {
    pub async fn start(addr: &str, copy_stream: CopyStream) -> Result<TcpServer> {
        let stop_token = CancellationToken::new();
        let task_tracker = TaskTracker::new();
        let listener = TcpListener::bind(addr).await?;

        {
            let stop_token = stop_token.clone();
            task_tracker.spawn(Self::run(stop_token, listener, copy_stream));
        }

        Ok(TcpServer {
            stop_token,
            task_tracker,
        })
    }

    async fn run(
        stop_token: CancellationToken,
        listener: TcpListener,
        copy_stream: CopyStream,
    ) -> Result<()> {
        loop {
            select! {
                _ = stop_token.cancelled() => {
                    break;
                }
                result = listener.accept() => {
                    let (mut stream, _) = result?;
                    let copy_stream = copy_stream.clone();
                    tokio::spawn(async move {
                        copy_stream.copy(&mut stream).await
                    });
                }
            }
        }

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

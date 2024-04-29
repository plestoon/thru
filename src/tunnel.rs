use anyhow::{anyhow, Error, Result};
use tokio::io::{AsyncRead, AsyncWrite, copy_bidirectional};
use url::Url;

use crate::config::Config;
use crate::transport::{TransportClient, TransportServer, TransportType};

#[derive(Debug)]
pub struct Tunnel {
    server: TransportServer,
    remote_client: TransportClient,
}

impl Tunnel {
    pub async fn open(from: TunnelEndpoint, to: TunnelEndpoint, config: Config) -> Result<Self> {
        let remote_client = TransportClient::new(to, config.clone()).await?;
        let copy_stream = CopyStream::new(remote_client.clone());
        let server = TransportServer::start(from, copy_stream, config.clone()).await?;

        Ok(Self {
            server,
            remote_client,
        })
    }

    pub async fn close(&self)  {
        self.server.stop().await;
        self.remote_client.disconnect().await;
    }
}

#[derive(Debug, Clone)]
pub struct TunnelEndpoint {
    pub transport_type: TransportType,
    pub addr: String,
}

impl TryFrom<&str> for TunnelEndpoint {
    type Error = Error;

    fn try_from(url: &str) -> Result<Self> {
        let url = Url::parse(url)?;
        let transport_type = TransportType::try_from(url.scheme())?;
        let host = url
            .host_str()
            .ok_or(anyhow!("invalid endpoint host: {}", url))?;
        let port = url
            .port()
            .ok_or(anyhow!("invalid endpoint port: {}", url))?;
        let addr = format!("{}:{}", host, port);

        Ok(TunnelEndpoint {
            transport_type,
            addr,
        })
    }
}

#[derive(Clone)]
pub struct CopyStream {
    remote_client: TransportClient,
}

impl CopyStream {
    pub fn new(remote_client: TransportClient) -> Self {
        Self { remote_client }
    }

    pub(crate) async fn copy<T>(&self, stream: &mut T) -> Result<()>
    where
        T: AsyncRead + AsyncWrite + Unpin + ?Sized,
    {
        let mut remote_stream = self.remote_client.connect().await?;
        copy_bidirectional(stream, &mut remote_stream).await?;

        Ok(())
    }
}

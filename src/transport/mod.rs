use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{anyhow, Error, Result};
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::config::Config;
use crate::transport::quic::{QuicClient, QuicServer, QuicStream};
use crate::transport::tcp::{TcpClient, TcpServer};
use crate::transport::udp::{UdpClient, UdpServer, UdpStream};
use crate::tunnel::{Tunnel, TunnelEndpoint};

mod quic;
mod tcp;
mod udp;

#[derive(Debug, Clone)]
pub enum TransportType {
    Tcp,
    Udp,
    Quic,
}

impl TryFrom<&str> for TransportType {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "tcp" => Ok(Self::Tcp),
            "udp" => Ok(Self::Udp),
            "quic" => Ok(Self::Quic),
            _ => Err(anyhow!("unknown transport type: {}", value)),
        }
    }
}

pin_project! {
    #[project = TransportProj]
    pub enum Transport {
        TcpTransport {#[pin] transport: TcpStream},
        UdpTransport {#[pin] transport: UdpStream},
        QuicTransport {#[pin] transport: QuicStream}
    }
}

impl AsyncRead for Transport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.project() {
            TransportProj::TcpTransport { transport } => AsyncRead::poll_read(transport, cx, buf),
            TransportProj::UdpTransport { transport } => {
                AsyncRead::poll_read(transport, cx, buf)
            }
            TransportProj::QuicTransport { transport } => AsyncRead::poll_read(transport, cx, buf),
        }
    }
}

impl AsyncWrite for Transport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.project() {
            TransportProj::TcpTransport { transport } => AsyncWrite::poll_write(transport, cx, buf),
            TransportProj::UdpTransport { transport } => {
                AsyncWrite::poll_write(transport, cx, buf)
            }
            TransportProj::QuicTransport { transport } => {
                AsyncWrite::poll_write(transport, cx, buf)
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.project() {
            TransportProj::TcpTransport { transport } => AsyncWrite::poll_flush(transport, cx),
            TransportProj::UdpTransport { transport } => {
                AsyncWrite::poll_flush(transport, cx)
            }
            TransportProj::QuicTransport { transport } => AsyncWrite::poll_flush(transport, cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            TransportProj::TcpTransport { transport } => AsyncWrite::poll_shutdown(transport, cx),
            TransportProj::UdpTransport { transport } => {
                AsyncWrite::poll_shutdown(transport, cx)
            }
            TransportProj::QuicTransport { transport } => AsyncWrite::poll_shutdown(transport, cx),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, std::io::Error>> {
        match self.project() {
            TransportProj::TcpTransport { transport } => {
                AsyncWrite::poll_write_vectored(transport, cx, bufs)
            }
            TransportProj::UdpTransport { transport } => {
                AsyncWrite::poll_write_vectored(transport, cx, bufs)
            }
            TransportProj::QuicTransport { transport } => {
                AsyncWrite::poll_write_vectored(transport, cx, bufs)
            }
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Self::TcpTransport { transport } => AsyncWrite::is_write_vectored(transport),
            Self::UdpTransport { transport } => AsyncWrite::is_write_vectored(transport),
            Self::QuicTransport { transport } => AsyncWrite::is_write_vectored(transport),
        }
    }
}

#[derive(Debug)]
pub enum TransportServer {
    TcpServer(TcpServer),
    UdpServer(UdpServer),
    QuicServer(QuicServer),
}

impl TransportServer {
    pub async fn start(tunnel: &Tunnel, config: &Config) -> Result<Self> {
        match tunnel.from.transport_type {
            TransportType::Tcp => Ok(TransportServer::TcpServer(
                TcpServer::start(tunnel, config).await?,
            )),
            TransportType::Udp => Ok(TransportServer::UdpServer(
                UdpServer::start(tunnel, config).await?,
            )),
            TransportType::Quic => Ok(TransportServer::QuicServer(
                QuicServer::start(tunnel, config).await?,
            )),
        }
    }

    pub async fn stop(&self) {
        match self {
            Self::TcpServer(server) => server.stop().await,
            Self::UdpServer(server) => server.stop().await,
            Self::QuicServer(server) => server.stop().await,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TransportClient {
    TcpClient(TcpClient),
    UdpClient(UdpClient),
    QuicClient(QuicClient),
}

impl TransportClient {
    pub async fn new(endpoint: &TunnelEndpoint, config: &Config) -> Result<Self> {
        match endpoint.transport_type {
            TransportType::Tcp => Ok(TransportClient::TcpClient(TcpClient::new(&endpoint.addr))),
            TransportType::Udp => Ok(TransportClient::UdpClient(
                UdpClient::new(&endpoint.addr).await?,
            )),
            TransportType::Quic => Ok(TransportClient::QuicClient(
                QuicClient::new(&endpoint.addr, config).await?,
            )),
        }
    }

    async fn connect(&self) -> Result<Transport> {
        match self {
            Self::TcpClient(client) => Ok(Transport::TcpTransport {
                transport: client.connect().await?,
            }),
            Self::UdpClient(client) => Ok(Transport::UdpTransport {
                transport: client.connect().await?,
            }),
            Self::QuicClient(client) => Ok(Transport::QuicTransport {
                transport: client.connect().await?,
            }),
        }
    }
}

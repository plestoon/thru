use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use anyhow::Result;
use bytes::{Bytes, BytesMut};
use futures_util::sink::SinkMapErr;
use futures_util::stream::{Map, SplitSink};
use futures_util::{SinkExt, StreamExt};
use pin_project_lite::pin_project;
use sender_sink::wrappers::{SinkError, UnboundedSenderSink};
use tokio::io::{copy_bidirectional, AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{lookup_host, UdpSocket};
use tokio::select;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::codec::BytesCodec;
use tokio_util::io::{CopyToBytes, SinkWriter, StreamReader};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tokio_util::udp::UdpFramed;

use crate::config::Config;
use crate::transport::TransportClient;
use crate::tunnel::Tunnel;
use crate::util::async_read_write::AsyncReadWrite;
use crate::util::copy_to_udp_frame::CopyToUdpFrame;

const MAX_DATAGRAM_SIZE: usize = 65_507;

#[derive(Debug)]
pub struct UdpServer {
    stop_token: CancellationToken,
    task_tracker: TaskTracker,
    dispatchers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Bytes>>>>,
}

impl UdpServer {
    pub async fn start(tunnel: &Tunnel, config: &Config) -> Result<Self> {
        let stop_token = CancellationToken::new();
        let task_tracker = TaskTracker::new();
        let socket = Arc::new(UdpSocket::bind(&tunnel.from.addr).await?);
        let dispatchers = Arc::new(Mutex::new(HashMap::new()));
        let client = TransportClient::new(&tunnel.to, &config).await?;

        {
            let stop_token = stop_token.clone();
            let task_tracker = task_tracker.clone();
            let dispatchers = dispatchers.clone();

            task_tracker.spawn(Self::run(stop_token, socket, dispatchers, client));
        }

        Ok(Self {
            stop_token,
            task_tracker,
            dispatchers,
        })
    }

    async fn run(
        stop_token: CancellationToken,
        socket: Arc<UdpSocket>,
        dispatchers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Bytes>>>>,
        client: TransportClient,
    ) -> Result<()> {
        loop {
            let mut buf = [0; MAX_DATAGRAM_SIZE];

            select! {
                _ = stop_token.cancelled() => {
                    break;
                }
                result = socket.recv_from(&mut buf) => {
                    let (n, addr) = result?;
                    Self::dispatch_packet(&buf[..n], socket.clone(), addr, dispatchers.clone(), client.clone());
                }
            }
        }

        Ok(())
    }

    fn dispatch_packet(
        buf: &[u8],
        socket: Arc<UdpSocket>,
        peer_addr: SocketAddr,
        dispatchers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Bytes>>>>,
        client: TransportClient,
    ) {
        let buf = Bytes::copy_from_slice(buf);
        let mut dispatchers = dispatchers.lock().unwrap();
        if let Some(dispatcher) = dispatchers.get(&peer_addr) {
            dispatcher.send(buf).unwrap(); // TODO
        } else {
            let (dispatcher_tx, accept_rx) = unbounded_channel::<Bytes>();
            let (accept_tx, mut dispatcher_rx) = unbounded_channel::<Bytes>();

            dispatcher_tx.send(buf).unwrap();
            dispatchers.insert(peer_addr, dispatcher_tx);
            let writer: UdpServerStreamWriter = SinkWriter::new(CopyToBytes::new(
                UnboundedSenderSink::from(accept_tx)
                    .sink_map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
            ));
            let reader: UdpServerStreamReader =
                StreamReader::new(UnboundedReceiverStream::from(accept_rx).map(|bytes| Ok(bytes)));
            let stream = UdpServerStream::new(reader, writer);

            tokio::spawn(async move {
                let _ = Self::handle_stream(stream, client).await;
            });

            {
                let socket = socket.clone();

                tokio::spawn(async move {
                    loop {
                        let buf = dispatcher_rx.recv().await.unwrap(); // TODO
                        socket.send_to(&buf, peer_addr).await.unwrap(); // TODO
                    }
                });
            }
        }
    }

    async fn handle_stream(
        mut stream: AsyncReadWrite<UdpServerStreamReader, UdpServerStreamWriter>,
        client: TransportClient,
    ) -> Result<()> {
        let mut remote_stream = client.connect().await?;
        copy_bidirectional(&mut stream, &mut remote_stream).await?;

        Ok(())
    }

    pub async fn stop(&self) {
        self.stop_token.cancel();
        self.task_tracker.close();
        self.task_tracker.wait().await;

        let mut dispatchers = self.dispatchers.lock().unwrap();
        dispatchers.clear();
    }
}

type UdpServerStreamWriter = SinkWriter<
    CopyToBytes<SinkMapErr<UnboundedSenderSink<Bytes>, fn(SinkError) -> std::io::Error>>,
>;
type UdpServerStreamReader = StreamReader<
    Map<UnboundedReceiverStream<Bytes>, fn(Bytes) -> Result<Bytes, std::io::Error>>,
    Bytes,
>;

type UdpServerStream = AsyncReadWrite<UdpServerStreamReader, UdpServerStreamWriter>;

#[derive(Debug, Clone)]
pub struct UdpClient {
    addr: SocketAddr, // TODO: resolve dns on recovery
}

impl UdpClient {
    pub async fn new(addr: &str) -> Result<Self> {
        let addr = lookup_host(addr)
            .await?
            .find(|addr| addr.is_ipv4())
            .unwrap();

        Ok(Self { addr })
    }

    pub async fn connect(&self) -> Result<UdpStream> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let framed = UdpFramed::new(socket, BytesCodec::new());
        let (sink, stream) = framed.split::<(Bytes, SocketAddr)>();
        let writer: UdpClientStreamWriter = SinkWriter::new(CopyToUdpFrame::new(sink, self.addr));
        let reader: UdpClientStreamReader =
            StreamReader::new(stream.map(|item| item.map(|(bytes, _)| bytes)));

        Ok(UdpStream::UdpClientStream {
            stream: UdpClientStream::new(reader, writer),
        })
    }
}

type UdpClientStreamWriter =
    SinkWriter<CopyToUdpFrame<SplitSink<UdpFramed<BytesCodec>, (Bytes, SocketAddr)>>>;
type UdpClientStreamReader = StreamReader<
    Map<
        futures_util::stream::SplitStream<UdpFramed<BytesCodec>>,
        fn(std::io::Result<(BytesMut, SocketAddr)>) -> std::io::Result<BytesMut>,
    >,
    BytesMut,
>;

type UdpClientStream = AsyncReadWrite<UdpClientStreamReader, UdpClientStreamWriter>;

pin_project! {
    #[project = UdpStreamProj]
    pub enum UdpStream {
        UdpServerStream{ #[pin] stream: UdpServerStream },
        UdpClientStream{ #[pin] stream: UdpClientStream }
    }
}

impl AsyncRead for UdpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.project() {
            UdpStreamProj::UdpServerStream { stream } => AsyncRead::poll_read(stream, cx, buf),
            UdpStreamProj::UdpClientStream { stream } => AsyncRead::poll_read(stream, cx, buf),
        }
    }
}

impl AsyncWrite for UdpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.project() {
            UdpStreamProj::UdpServerStream { stream } => AsyncWrite::poll_write(stream, cx, buf),
            UdpStreamProj::UdpClientStream { stream } => AsyncWrite::poll_write(stream, cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.project() {
            UdpStreamProj::UdpServerStream { stream } => AsyncWrite::poll_flush(stream, cx),
            UdpStreamProj::UdpClientStream { stream } => AsyncWrite::poll_flush(stream, cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            UdpStreamProj::UdpServerStream { stream } => AsyncWrite::poll_shutdown(stream, cx),
            UdpStreamProj::UdpClientStream { stream } => AsyncWrite::poll_shutdown(stream, cx),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, std::io::Error>> {
        match self.project() {
            UdpStreamProj::UdpServerStream { stream } => {
                AsyncWrite::poll_write_vectored(stream, cx, bufs)
            }
            UdpStreamProj::UdpClientStream { stream } => {
                AsyncWrite::poll_write_vectored(stream, cx, bufs)
            }
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Self::UdpServerStream { stream } => AsyncWrite::is_write_vectored(stream),
            Self::UdpClientStream { stream } => AsyncWrite::is_write_vectored(stream),
        }
    }
}
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use futures_util::sink::SinkMapErr;
use futures_util::stream::{Map, SplitSink};
use sender_sink::wrappers::{SinkError, UnboundedSenderSink};
use tokio::io::{Join, join};
use tokio::net::{lookup_host, UdpSocket};
use tokio::select;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::codec::BytesCodec;
use tokio_util::io::{CopyToBytes, SinkWriter, StreamReader};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tokio_util::udp::UdpFramed;

use crate::config::Config;
use crate::tunnel::CopyStream;
use crate::util::copy_to_udp_frame::CopyToUdpFrame;
use crate::util::idle_timeout_stream::IdleTimeoutRead;

const MAX_DATAGRAM_SIZE: usize = 65_507;

#[derive(Debug)]
pub struct UdpServer {
    stop_token: CancellationToken,
    task_tracker: TaskTracker,
}

impl UdpServer {
    pub async fn start(addr: &str, copy_stream: CopyStream, config: Config) -> Result<Self> {
        let stop_token = CancellationToken::new();
        let task_tracker = TaskTracker::new();
        let socket = Arc::new(UdpSocket::bind(addr).await?);

        {
            let stop_token = stop_token.clone();
            task_tracker.spawn(Self::run(stop_token, socket, copy_stream, config));
        }

        Ok(Self {
            stop_token,
            task_tracker,
        })
    }

    async fn run(
        stop_token: CancellationToken,
        socket: Arc<UdpSocket>,
        copy_stream: CopyStream,
        config: Config,
    ) -> Result<()> {
        let dispatchers = Arc::new(Mutex::new(HashMap::new()));

        loop {
            let mut buf = [0; MAX_DATAGRAM_SIZE];

            select! {
                _ = stop_token.cancelled() => {
                    break;
                }
                result = socket.recv_from(&mut buf) => {
                    let (n, addr) = result?;
                    let copy_stream= copy_stream.clone();
                    Self::dispatch(&buf[..n], socket.clone(), addr, dispatchers.clone(), copy_stream, &config);
                }
            }
        }

        {
            dispatchers.lock().unwrap().clear();
        }

        Ok(())
    }

    fn dispatch(
        buf: &[u8],
        socket: Arc<UdpSocket>,
        peer_addr: SocketAddr,
        dispatchers_lock: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Bytes>>>>,
        copy_stream: CopyStream,
        config: &Config,
    ) {
        let buf = Bytes::copy_from_slice(buf);
        let mut dispatchers = dispatchers_lock.lock().unwrap();
        if let Some(dispatcher) = dispatchers.get(&peer_addr) {
            if dispatcher.send(buf).is_err() {
                dispatchers.remove(&peer_addr);
            }
        } else {
            let (dispatcher_tx, accept_rx) = unbounded_channel::<Bytes>();
            let (accept_tx, dispatcher_rx) = unbounded_channel::<Bytes>();

            dispatcher_tx.send(buf).unwrap();
            dispatchers.insert(peer_addr, dispatcher_tx);
            let writer: UdpServerStreamWriter = SinkWriter::new(CopyToBytes::new(
                UnboundedSenderSink::from(accept_tx)
                    .sink_map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
            ));
            let reader: UdpServerStreamReader = IdleTimeoutRead::new(
                StreamReader::new(UnboundedReceiverStream::from(accept_rx).map(|bytes| Ok(bytes))),
                config.udp_max_idle_timeout,
            );

            let mut stream = join(reader, writer);
            tokio::spawn(async move {
                copy_stream.copy(&mut stream).await
            });

            {
                let dispatchers = dispatchers_lock.clone();
                let socket = socket.clone();

                tokio::spawn(Self::copy_backward(
                    dispatchers,
                    dispatcher_rx,
                    socket,
                    peer_addr,
                ));
            }
        }
    }

    async fn copy_backward(
        dispatchers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Bytes>>>>,
        mut dispatcher_rx: UnboundedReceiver<Bytes>,
        socket: Arc<UdpSocket>,
        peer_addr: SocketAddr,
    ) -> Result<()> {
        loop {
            match dispatcher_rx.recv().await {
                Some(buf) => {
                    if socket.send_to(&buf, peer_addr).await.is_err() {
                        break;
                    }
                }
                None => break,
            }
        }

        let mut dispatchers = dispatchers.lock().unwrap();
        dispatchers.remove(&peer_addr);

        Ok(())
    }

    pub async fn stop(&self) {
        self.stop_token.cancel();
        self.task_tracker.close();
        self.task_tracker.wait().await;
    }
}

type UdpServerStreamWriter = SinkWriter<
    CopyToBytes<SinkMapErr<UnboundedSenderSink<Bytes>, fn(SinkError) -> std::io::Error>>,
>;
type UdpServerStreamReader = IdleTimeoutRead<
    StreamReader<
        Map<UnboundedReceiverStream<Bytes>, fn(Bytes) -> Result<Bytes, std::io::Error>>,
        Bytes,
    >,
>;

#[derive(Debug, Clone)]
pub struct UdpClient {
    addr: SocketAddr,
    config: Config,
}

impl UdpClient {
    pub async fn new(addr: &str, config: Config) -> Result<Self> {
        let addr = lookup_host(addr)
            .await?
            .find(|addr| addr.is_ipv4())
            .unwrap();

        Ok(Self { addr, config })
    }

    pub async fn connect(&self) -> Result<UdpClientStream> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let framed = UdpFramed::new(socket, BytesCodec::new());
        let (sink, stream) = framed.split::<(Bytes, SocketAddr)>();
        let writer: UdpClientStreamWriter = SinkWriter::new(CopyToUdpFrame::new(sink, self.addr));
        let reader: UdpClientStreamReader = IdleTimeoutRead::new(
            StreamReader::new(stream.map(|item| item.map(|(bytes, _)| bytes))),
            self.config.udp_max_idle_timeout,
        );

        Ok(join(reader, writer))
    }
}

type UdpClientStreamWriter =
    SinkWriter<CopyToUdpFrame<SplitSink<UdpFramed<BytesCodec>, (Bytes, SocketAddr)>>>;
type UdpClientStreamReader = IdleTimeoutRead<
    StreamReader<
        Map<
            futures_util::stream::SplitStream<UdpFramed<BytesCodec>>,
            fn(std::io::Result<(BytesMut, SocketAddr)>) -> std::io::Result<BytesMut>,
        >,
        BytesMut,
    >,
>;

pub type UdpClientStream = Join<UdpClientStreamReader, UdpClientStreamWriter>;

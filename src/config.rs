use std::time::Duration;

const QUIC_MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const QUIC_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);
const QUIC_RETRY_INTERVAL: Duration = Duration::from_secs(5);
const QUIC_STREAM_RECEIVE_WINDOW: u64 = 1024 * 1024 * 2;
const QUIC_SEND_WINDOW: u64 = QUIC_STREAM_RECEIVE_WINDOW * 8;

const UDP_MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct Config {
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub tls_peer_cert_path: Option<String>,
    pub quic_max_idle_timeout: Duration,
    pub quic_keep_alive_interval: Duration,
    pub quic_retry_interval: Duration,
    pub quic_stream_receive_window: u64,
    pub quic_send_window: u64,
    pub udp_max_idle_timeout: Duration,
}

impl Config {
    pub fn new(
        tls_cert_path: Option<String>,
        tls_key_path: Option<String>,
        tls_peer_cert_path: Option<String>,
    ) -> Self {
        Self {
            tls_cert_path,
            tls_key_path,
            tls_peer_cert_path,
            quic_max_idle_timeout: QUIC_MAX_IDLE_TIMEOUT,
            quic_keep_alive_interval: QUIC_KEEP_ALIVE_INTERVAL,
            quic_retry_interval: QUIC_RETRY_INTERVAL,
            quic_stream_receive_window: QUIC_STREAM_RECEIVE_WINDOW,
            quic_send_window: QUIC_SEND_WINDOW,
            udp_max_idle_timeout: UDP_MAX_IDLE_TIMEOUT,
        }
    }
}

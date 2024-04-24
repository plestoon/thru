use anyhow::{anyhow, Error, Result};
use url::Url;

use crate::transport::TransportType;

#[derive(Debug, Clone)]
pub struct Tunnel {
    pub from: TunnelEndpoint,
    pub to: TunnelEndpoint,
}

impl Tunnel {
    pub fn new(from: TunnelEndpoint, to: TunnelEndpoint) -> Self {
        Self { from, to }
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

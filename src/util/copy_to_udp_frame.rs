use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::Sink;
use pin_project_lite::pin_project;

pin_project! {
    #[derive(Debug)]
    pub struct CopyToUdpFrame<S> {
        #[pin]
        inner: S,
        addr: SocketAddr
    }
}

impl<S> CopyToUdpFrame<S> {
    pub fn new(inner: S, addr: SocketAddr) -> Self {
        Self { inner, addr }
    }
}

impl<'a, S> Sink<&'a [u8]> for CopyToUdpFrame<S>
where
    S: Sink<(Bytes, SocketAddr)>,
{
    type Error = S::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: &'a [u8]) -> Result<(), Self::Error> {
        let addr = self.addr.clone();

        self.project()
            .inner
            .start_send((Bytes::copy_from_slice(item), addr))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

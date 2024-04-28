use quinn::AsyncTimer;
use std::io::ErrorKind;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, Error, ReadBuf, Result};
use tokio::time::{sleep, Instant, Sleep};

pub struct IdleTimeoutRead<T> {
    inner: T,
    timeout: Duration,
    timer: Pin<Box<Sleep>>,
}

impl<T> IdleTimeoutRead<T>
where
    T: AsyncRead + Unpin,
{
    pub fn new(inner: T, timeout: Duration) -> Self {
        Self {
            inner,
            timeout,
            timer: Box::pin(sleep(timeout)),
        }
    }
}

impl<T> AsyncRead for IdleTimeoutRead<T>
where
    T: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        if self.timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(Error::from(ErrorKind::TimedOut)));
        }

        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(r) => {
                if r.is_ok() && !buf.filled().is_empty() {
                    let timeout = self.timeout;
                    self.timer.as_mut().reset(Instant::now() + timeout);
                }
                Poll::Ready(r)
            }
        }
    }
}

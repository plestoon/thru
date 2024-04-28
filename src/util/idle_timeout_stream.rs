use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::task::AtomicWaker;
use tokio::io::{AsyncRead, Error, ReadBuf, Result};
use tokio::time::sleep;

use crate::util::time_util::epoch_now_secs;

pub struct IdleTimeoutRead<T> {
    inner: T,
    last_active_at: Arc<AtomicU64>, // epoch seconds
    timed_out: Arc<AtomicBool>,
    waker: Arc<AtomicWaker>,
}

impl<T> IdleTimeoutRead<T>
where
    T: AsyncRead + Unpin + 'static,
{
    pub fn new(inner: T, timeout: u64) -> Self {
        let last_active_at = Arc::new(AtomicU64::new(epoch_now_secs()));
        let timed_out = Arc::new(AtomicBool::new(false));
        let waker = Arc::new(AtomicWaker::new());

        {
            let last_active_at = last_active_at.clone();
            let timed_out = timed_out.clone();
            let waker = waker.clone();

            tokio::spawn(Self::monitor_timeout(
                timeout,
                last_active_at,
                timed_out,
                waker,
            ));
        }

        Self {
            inner,
            last_active_at,
            timed_out,
            waker,
        }
    }

    async fn monitor_timeout(
        timeout: u64,
        last_active_at: Arc<AtomicU64>,
        timed_out: Arc<AtomicBool>,
        waker: Arc<AtomicWaker>,
    ) {
        // TODO: stop the loop on EOF or Err
        loop {
            sleep(Duration::from_secs(1)).await;
            let now = epoch_now_secs();
            let last_active_at = last_active_at.load(Ordering::Relaxed);
            if now > last_active_at + timeout {
                timed_out.store(true, Ordering::Relaxed);
                waker.wake();

                break;
            }
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
        if self.timed_out.fetch_and(true, Ordering::Relaxed) {
            return Poll::Ready(Err(Error::from(ErrorKind::TimedOut)));
        }

        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Pending => {
                self.waker.register(cx.waker());
                Poll::Pending
            }
            Poll::Ready(r) => {
                if r.is_ok() {
                    self.last_active_at
                        .store(epoch_now_secs(), Ordering::Relaxed);
                }

                Poll::Ready(r)
            }
        }
    }
}

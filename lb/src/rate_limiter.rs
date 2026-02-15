use std::{
    io::Result,
    num::NonZeroU32,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use governor::{
    RateLimiter,
    clock::{Clock, DefaultClock},
    state::{InMemoryState, NotKeyed},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::{Instant, Sleep},
};

/// A wrapper around a generic IO stream that enforces bandwidth limits.
pub struct RateLimitedStream<T> {
    inner: T,
    limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    sleep: Option<Pin<Box<Sleep>>>,
    pending_bytes: u32,
}

impl<T> RateLimitedStream<T> {
    pub fn new(inner: T, limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>) -> Self {
        Self {
            inner,
            limiter,
            sleep: None,
            pending_bytes: 0,
        }
    }

    fn poll_pending_bytes(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if self.pending_bytes == 0 {
            return Poll::Ready(());
        }

        let nonzero = match NonZeroU32::new(self.pending_bytes) {
            Some(n) => n,
            None => {
                self.pending_bytes = 0;
                return Poll::Ready(());
            }
        };

        match self.limiter.check_n(nonzero) {
            Ok(Ok(_)) | Err(_) => {
                self.pending_bytes = 0;
                Poll::Ready(())
            }
            Ok(Err(not_until)) => {
                let wait_time = not_until.wait_time_from(DefaultClock::default().now());
                let mut sleep = Box::pin(tokio::time::sleep_until(Instant::now() + wait_time));
                if sleep.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(());
                }
                self.sleep = Some(sleep);
                Poll::Pending
            }
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for RateLimitedStream<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        let mut_rl = self.get_mut();
        if let Some(ref mut sleep) = mut_rl.sleep {
            std::task::ready!(sleep.as_mut().poll(cx));
            mut_rl.sleep = None;
        }

        std::task::ready!(mut_rl.poll_pending_bytes(cx));

        let before = buf.filled().len();
        let poll = Pin::new(&mut mut_rl.inner).poll_read(cx, buf);
        let after = buf.filled().len();
        let diff = after - before;

        if diff > 0 {
            mut_rl.pending_bytes = mut_rl.pending_bytes.saturating_add(diff as u32);
        }

        poll
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for RateLimitedStream<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

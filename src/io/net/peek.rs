use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Peek<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    io_request: Option<IoRequest>,
}

impl<'buf> Peek<'buf> {
    pub fn new(fd: RawFd, buf: &'buf mut [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request: None,
        }
    }
}

impl<'buf> Future for Peek<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.peek(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len(),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct PeekWithDeadline<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'buf> PeekWithDeadline<'buf> {
    pub fn new(fd: RawFd, buf: &'buf mut [u8], deadline: Instant) -> Self {
        Self {
            fd,
            buf,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'buf> Future for PeekWithDeadline<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.peek(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len(),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

pub trait AsyncPeek: AsRawFd {
    #[inline(always)]
    async fn peek(&mut self, buf: &mut [u8]) -> Result<usize> {
        Peek::new(self.as_raw_fd(), buf).await
    }

    #[inline(always)]
    async fn peek_with_deadline(&mut self, buf: &mut [u8], deadline: Instant) -> Result<usize> {
        PeekWithDeadline::new(self.as_raw_fd(), buf, deadline).await
    }

    #[inline(always)]
    async fn peek_with_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        self.peek_with_deadline(buf, Instant::now() + timeout).await
    }

    #[inline(always)]
    async fn peek_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut peeked = 0;

        while peeked < buf.len() {
            peeked += self.peek(&mut buf[peeked..]).await?;
        }
        Ok(())
    }

    #[inline(always)]
    async fn peek_exact_with_deadline(&mut self, buf: &mut [u8], deadline: Instant) -> Result<()> {
        let mut peeked = 0;

        while peeked < buf.len() {
            peeked += self.peek_with_deadline(&mut buf[peeked..], deadline).await?;
        }
        Ok(())
    }

    #[inline(always)]
    async fn peek_exact_with_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()> {
        self.peek_exact_with_deadline(buf, Instant::now() + timeout).await
    }
}

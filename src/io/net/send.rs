use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Send<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    io_request: Option<IoRequest>,
}

impl<'buf> Send<'buf> {
    pub fn new(fd: RawFd, buf: &'buf [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request: None,
        }
    }
}

impl<'buf> Future for Send<'buf> {
    type Output = Result<usize>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.send(
                this.fd,
                this.buf.as_ptr(),
                this.buf.len(),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct SendWithDeadline<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'buf> SendWithDeadline<'buf> {
    pub fn new(fd: RawFd, buf: &'buf [u8], deadline: Instant) -> Self {
        Self {
            fd,
            buf,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'buf> Future for SendWithDeadline<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.send(
                this.fd,
                this.buf.as_ptr(),
                this.buf.len(),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

pub trait AsyncSend: AsRawFd {
    #[inline(always)]
    async fn send(&mut self, buf: &[u8]) -> Result<usize> {
        Send::new(self.as_raw_fd(), buf).await
    }

    #[inline(always)]
    async fn send_with_deadline(&mut self, buf: &[u8], deadline: Instant) -> Result<usize> {
        SendWithDeadline::new(self.as_raw_fd(), buf, deadline).await
    }

    #[inline(always)]
    async fn send_with_timeout(&mut self, buf: &[u8], timeout: Duration) -> Result<usize> {
        SendWithDeadline::new(self.as_raw_fd(), buf, Instant::now() + timeout).await
    }

    #[inline(always)]
    async fn send_all(&mut self, buf: &[u8]) -> Result<()> {
        let mut sent = 0;
        while sent < buf.len() {
            sent += self.send(&buf[sent..]).await?;
        }
        Ok(())
    }

    #[inline(always)]
    async fn send_all_with_deadline(&mut self, buf: &[u8], deadline: Instant) -> Result<()> {
        let mut sent = 0;
        while sent < buf.len() {
            sent += self.send_with_deadline(&buf[sent..], deadline).await?;
        }
        Ok(())
    }

    #[inline(always)]
    async fn send_all_with_timeout(&mut self, buf: &[u8], timeout: Duration) -> Result<()> {
        self.send_all_with_deadline(buf, Instant::now() + timeout).await
    }
}
use std::future::Future;
use std::io::Result;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsRawFd, RawFd, MessageRecvHeader};
use crate::io::worker::{local_worker, IoWorker};
use crate::messages::BUG;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct PeekFrom<'fut> {
    fd: RawFd,
    msg_header: MessageRecvHeader<'fut>,
    addr: &'fut mut SockAddr,
    io_request: Option<IoRequest>,
}

impl<'fut> PeekFrom<'fut> {
    pub fn new(fd: RawFd, buf: &'fut mut [u8], addr: &'fut mut SockAddr) -> Self {
        Self {
            fd,
            msg_header: MessageRecvHeader::new(buf),
            addr,
            io_request: None,
        }
    }
}

impl<'fut> Future for PeekFrom<'fut> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.peek_from(
                this.fd,
                this.msg_header.get_os_message_header_ptr(this.addr),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct PeekFromWithDeadline<'fut> {
    fd: RawFd,
    msg_header: MessageRecvHeader<'fut>,
    addr: &'fut mut SockAddr,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'fut> PeekFromWithDeadline<'fut> {
    pub fn new(fd: RawFd, buf: &'fut mut [u8], addr: &'fut mut SockAddr, deadline: Instant) -> Self {
        Self {
            fd,
            msg_header: MessageRecvHeader::new(buf),
            addr,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'fut> Future for PeekFromWithDeadline<'fut> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.peek_from(
                this.fd,
                this.msg_header.get_os_message_header_ptr(this.addr),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

pub trait AsyncPeekFrom: AsRawFd {
    #[inline(always)]
    async fn peek_from(
        &mut self,
        buf: &mut [u8]
    ) -> Result<(usize, SocketAddr)> {
    let mut sock_addr = unsafe { std::mem::zeroed() };
    let n = PeekFrom::new(self.as_raw_fd(), buf, &mut sock_addr).await?;

    Ok((n, sock_addr.as_socket().expect(BUG)))
    }

    #[inline(always)]
    async fn peek_from_with_deadline(
        &mut self,
        buf: &mut [u8],
        deadline: Instant
    ) -> Result<(usize, SocketAddr)> {
    let mut sock_addr = unsafe { std::mem::zeroed() };
    let n = PeekFromWithDeadline::new(self.as_raw_fd(), buf, &mut sock_addr, deadline).await?;

    Ok((n, sock_addr.as_socket().expect(BUG)))
    }

    #[inline(always)]
    async fn peek_from_with_timeout(
        &mut self,
        buf: &mut [u8],
        timeout: Duration
    ) -> Result<(usize, SocketAddr)> {
        self.peek_from_with_deadline(buf, Instant::now() + timeout).await
    }
}
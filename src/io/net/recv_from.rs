use std::future::Future;
use std::io::Result;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsFd, Fd, MessageRecvHeader};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::AsyncPollFd;
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct RecvFrom<'buf> {
    fd: Fd,
    msg_header: MessageRecvHeader<'buf>,
    io_request: Option<IoRequest>,
}

impl<'buf> RecvFrom<'buf> {
    #[inline(always)]
    pub fn new(fd: Fd, buf: &'buf mut [u8]) -> Self {
        let s = Self {
            fd,
            msg_header: MessageRecvHeader::new(buf),
            io_request: None,
        };

        s
    }
}

impl<'buf> Future for RecvFrom<'buf> {
    type Output = Result<(usize, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.recv_from(
                this.fd,
                this.msg_header.get_os_message_header_ptr(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            (ret, this.msg_header.socket_addr().clone())
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct RecvFromWithDeadline<'buf> {
    fd: Fd,
    msg_header: MessageRecvHeader<'buf>,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'buf> RecvFromWithDeadline<'buf> {
    #[inline(always)]
    pub fn new(fd: Fd, buf: &'buf mut [u8], deadline: Instant) -> Self {
        Self {
            fd,
            msg_header: MessageRecvHeader::new(buf),
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'buf> Future for RecvFromWithDeadline<'buf> {
    type Output = Result<(usize, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.recv_from(
                this.fd,
                this.msg_header.get_os_message_header_ptr(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            (ret, this.msg_header.socket_addr().clone())
        ));
    }
}

pub trait AsyncRecvFrom: AsFd + AsyncPollFd {
    #[inline(always)]
    async fn recv_from<'a>(&mut self, buf: &'a mut [u8]) -> Result<(usize, SocketAddr)> {
        let (n, addr) = RecvFrom::new(self.as_raw_fd(), buf).await?;
        Ok((
            n,
            addr.as_socket()
                .expect("[BUG] Invalid socket address. Please report the bug."),
        ))
    }

    #[inline(always)]
    async fn recv_from_with_deadline<'a>(
        &mut self,
        buf: &'a mut [u8],
        deadline: Instant,
    ) -> Result<(usize, SocketAddr)> {
        let (n, addr) = RecvFromWithDeadline::new(self.as_raw_fd(), buf, deadline).await?;
        Ok((
            n,
            addr.as_socket()
                .expect("[BUG] Invalid socket address. Please report the bug."),
        ))
    }

    #[inline(always)]
    async fn recv_from_with_timeout<'a>(
        &mut self,
        buf: &'a mut [u8],
        duration: std::time::Duration,
    ) -> Result<(usize, SocketAddr)> {
        self.recv_from_with_deadline(buf, Instant::now() + duration)
            .await
    }
}

pub trait AsyncRecvFromPath: AsFd {
    #[inline(always)]
    async fn recv_from<'buf>(&mut self, buf: &'buf mut [u8]) -> Result<(usize, PathBuf)> {
        let (n, addr) = RecvFrom::new(self.as_raw_fd(), buf).await?;
        let path = addr
            .as_pathname()
            .expect("[BUG] Invalid socket address. Please report the bug.")
            .to_path_buf();
        Ok((n, path))
    }

    #[inline(always)]
    async fn recv_from_with_deadline<'buf>(
        &mut self,
        buf: &'buf mut [u8],
        deadline: Instant,
    ) -> Result<(usize, PathBuf)> {
        let (n, addr) = RecvFromWithDeadline::new(self.as_raw_fd(), buf, deadline).await?;
        let path = addr
            .as_pathname()
            .expect("[BUG] Invalid socket address. Please report the bug.")
            .to_path_buf();
        Ok((n, path))
    }

    #[inline(always)]
    async fn recv_from_with_timeout<'buf>(
        &mut self,
        buf: &'buf mut [u8],
        duration: std::time::Duration,
    ) -> Result<(usize, PathBuf)> {
        self.recv_from_with_deadline(buf, Instant::now() + duration)
            .await
    }
}

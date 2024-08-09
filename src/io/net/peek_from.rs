use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{Fd, MessageRecvHeader};
use crate::io::worker::{local_worker, IoWorker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct PeekFrom<'buf> {
    fd: Fd,
    msg_header: MessageRecvHeader<'buf>,
    io_request: Option<IoRequest>,
}

impl<'buf> PeekFrom<'buf> {
    pub fn new(fd: Fd, buf: &'buf mut [u8]) -> Self {
        Self {
            fd,
            msg_header: MessageRecvHeader::new(buf),
            io_request: None,
        }
    }
}

impl<'buf> Future for PeekFrom<'buf> {
    type Output = Result<(usize, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.peek_from(
                this.fd,
                this.msg_header.get_os_message_header_ptr(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            (ret, this.msg_header.socket_addr().clone())
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct PeekFromWithDeadline<'buf> {
    fd: Fd,
    msg_header: MessageRecvHeader<'buf>,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'buf> PeekFromWithDeadline<'buf> {
    pub fn new(fd: Fd, buf: &'buf mut [u8], deadline: Instant) -> Self {
        Self {
            fd,
            msg_header: MessageRecvHeader::new(buf),
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'buf> Future for PeekFromWithDeadline<'buf> {
    type Output = Result<(usize, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.peek_from(
                this.fd,
                this.msg_header.get_os_message_header_ptr(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            (ret, this.msg_header.socket_addr().clone())
        ));
    }
}

#[macro_export]
macro_rules! generate_peek_from {
    () => {
        #[inline(always)]
        pub async fn peek_from<'a>(
            &mut self,
            buf: &'a mut [u8],
        ) -> std::io::Result<(usize, std::net::SocketAddr)> {
            let (n, addr) = crate::io::PeekFrom::new(self.as_raw_fd(), buf).await?;
            Ok((
                n,
                addr.as_socket()
                    .expect("[BUG] Invalid socket address. Please report the bug."),
            ))
        }

        #[inline(always)]
        pub async fn peek_from_with_deadline<'a>(
            &mut self,
            buf: &'a mut [u8],
            deadline: std::time::Instant,
        ) -> std::io::Result<(usize, std::net::SocketAddr)> {
            let (n, addr) =
                crate::io::PeekFromWithDeadline::new(self.as_raw_fd(), buf, deadline).await?;
            Ok((
                n,
                addr.as_socket()
                    .expect("[BUG] Invalid socket address. Please report the bug."),
            ))
        }

        #[inline(always)]
        pub async fn peek_from_with_timeout<'a>(
            &mut self,
            buf: &'a mut [u8],
            duration: std::time::Duration,
        ) -> std::io::Result<(usize, std::net::SocketAddr)> {
            self.peek_from_with_deadline(buf, std::time::Instant::now() + duration)
                .await
        }
    };
}

#[macro_export]
macro_rules! generate_peek_from_unix {
    () => {
        #[inline(always)]
        pub async fn peek_from<'a>(
            &mut self,
            buf: &'a mut [u8],
        ) -> std::io::Result<(usize, std::os::unix::net::SocketAddr)> {
            let (n, addr) = crate::io::PeekFrom::new(self.as_raw_fd(), buf).await?;
            Ok((
                n,
                addr.as_unix()
                    .expect("[BUG] Invalid socket address. Please report the bug."),
            ))
        }

        #[inline(always)]
        pub async fn peek_from_with_deadline<'a>(
            &mut self,
            buf: &'a mut [u8],
            deadline: std::time::Instant,
        ) -> std::io::Result<(usize, std::os::unix::net::SocketAddr)> {
            let (n, addr) =
                crate::io::PeekFromWithDeadline::new(self.as_raw_fd(), buf, deadline).await?;
            Ok((
                n,
                addr.as_unix()
                    .expect("[BUG] Invalid socket address. Please report the bug."),
            ))
        }

        #[inline(always)]
        pub async fn peek_from_with_timeout<'a>(
            &mut self,
            buf: &'a mut [u8],
            duration: std::time::Duration,
        ) -> std::io::Result<(usize, std::os::unix::net::SocketAddr)> {
            self.peek_from_with_deadline(buf, std::time::Instant::now() + duration)
                .await
        }
    };
}

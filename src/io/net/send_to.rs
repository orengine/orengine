use std::future::Future;
use std::io::Result;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{Fd, MessageHeader};
use crate::io::worker::{local_worker, IoWorker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct SendTo<'buf> {
    fd: Fd,
    message_header: MessageHeader<'buf>,
    io_request: Option<IoRequest>,
}

impl<'buf> SendTo<'buf> {
    pub fn new(fd: Fd, buf: &'buf [u8], addr: SocketAddr) -> Self {
        Self {
            fd,
            message_header: MessageHeader::new_for_send_to(buf, addr),
            io_request: None,
        }
    }
}

impl<'buf> Future for SendTo<'buf> {
    type Output = Result<usize>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.send_to(
                this.fd,
                this.message_header.get_os_message_header_ptr(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct SendToWithDeadline<'buf> {
    fd: Fd,
    message_header: MessageHeader<'buf>,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'a> SendToWithDeadline<'a> {
    pub fn new(fd: Fd, buf: &'a [u8], addr: SocketAddr, deadline: Instant) -> Self {
        Self {
            fd,
            message_header: MessageHeader::new_for_send_to(buf, addr),
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'a> Future for SendToWithDeadline<'a> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.send_to(
                this.fd,
                this.message_header.get_os_message_header_ptr(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[macro_export]
macro_rules! generate_send_to {
    () => {
        #[inline(always)]
        pub async fn send_to<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A) -> Result<usize> {
            crate::io::SendTo::new(self.fd, buf, crate::utils::addr_from_to_socket_addrs(addr)?)
                .await
        }

        #[inline(always)]
        pub async fn send_to_with_deadline<A: ToSocketAddrs>(
            &mut self,
            buf: &[u8],
            addr: A,
            deadline: Instant,
        ) -> Result<usize> {
            crate::io::SendToWithDeadline::new(
                self.fd,
                buf,
                crate::utils::addr_from_to_socket_addrs(addr)?,
                deadline,
            )
            .await
        }

        #[inline(always)]
        pub async fn send_to_with_timeout<A: ToSocketAddrs>(
            &mut self,
            buf: &[u8],
            addr: A,
            duration: Duration,
        ) -> Result<usize> {
            let deadline = Instant::now() + duration;
            self.send_to_with_deadline(buf, addr, deadline).await
        }
    };
}

#[macro_export]
macro_rules! generate_send_all_to {
    () => {
        #[inline(always)]
        pub async fn send_all_to<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A) -> Result<()> {
            let mut sent = 0;
            let socket_addr = crate::utils::addr_from_to_socket_addrs(addr)?;

            while sent < buf.len() {
                sent += crate::io::SendTo::new(self.fd, buf, socket_addr).await?;
            }

            Ok(())
        }

        #[inline(always)]
        pub async fn send_all_to_with_deadline<A: ToSocketAddrs>(
            &mut self,
            buf: &[u8],
            addr: A,
            deadline: Instant,
        ) -> Result<()> {
            let mut sent = 0;
            let socket_addr = crate::utils::addr_from_to_socket_addrs(addr)?;

            while sent < buf.len() {
                sent +=
                    crate::io::SendToWithDeadline::new(self.fd, buf, socket_addr, deadline).await?;
            }

            Ok(())
        }

        #[inline(always)]
        pub async fn send_all_to_with_timeout<A: ToSocketAddrs>(
            &mut self,
            buf: &[u8],
            addr: A,
            duration: Duration,
        ) -> Result<()> {
            let deadline = Instant::now() + duration;
            self.send_all_to_with_deadline(buf, addr, deadline).await
        }
    };
}

use std::future::Future;
use std::io;
use std::io::{ErrorKind, Result};
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsRawFd, RawFd, MessageSendHeader};
use crate::io::worker::{local_worker, IoWorker};

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct SendTo<'fut> {
    fd: RawFd,
    message_header: MessageSendHeader<'fut>,
    addr: &'fut SockAddr,
    io_request: Option<IoRequest>,
}

impl<'fut> SendTo<'fut> {
    pub fn new(fd: RawFd, buf: &'fut [u8], addr: &'fut SockAddr) -> Self {
        Self {
            fd,
            message_header: MessageSendHeader::new(buf),
            addr,
            io_request: None,
        }
    }
}

impl<'fut> Future for SendTo<'fut> {
    type Output = Result<usize>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.send_to(
                this.fd,
                this.message_header.get_os_message_header_ptr(this.addr),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct SendToWithDeadline<'fut> {
    fd: RawFd,
    message_header: MessageSendHeader<'fut>,
    addr: &'fut SockAddr,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'fut> SendToWithDeadline<'fut> {
    pub fn new(fd: RawFd, buf: &'fut [u8], addr: &'fut SockAddr, deadline: Instant) -> Self {
        Self {
            fd,
            message_header: MessageSendHeader::new(buf),
            addr,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'fut> Future for SendToWithDeadline<'fut> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.send_to(
                this.fd,
                this.message_header.get_os_message_header_ptr(this.addr),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[inline(always)]
// TODO say     /// It is possible for `addr` to yield multiple addresses, but `send_to`
//              /// will only send data to the first address yielded by `addr`.
fn sock_addr_from_to_socket_addr<A: ToSocketAddrs>(to_addr: A) -> Result<SockAddr> {
    match to_addr.to_socket_addrs()?.next() {
        Some(addr) => Ok(SockAddr::from(addr)),
        None => {
            Err(io::Error::new(ErrorKind::InvalidInput, "no addresses to send data to"))
        }
    }
}

pub trait AsyncSendTo: AsRawFd {
    #[inline(always)]
    async fn send_to<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A) -> Result<usize> {
        SendTo::new(
            self.as_raw_fd(),
            buf,
            &sock_addr_from_to_socket_addr(addr)?
        ).await
    }

    #[inline(always)]
    async fn send_to_with_deadline<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A, deadline: Instant) -> Result<usize> {
        SendToWithDeadline::new(
            self.as_raw_fd(),
            buf,
            &sock_addr_from_to_socket_addr(addr)?,
            deadline
        ).await
    }

    #[inline(always)]
    async fn send_to_with_timeout<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A, timeout: Duration) -> Result<usize> {
        SendToWithDeadline::new(
            self.as_raw_fd(),
            buf,
            &sock_addr_from_to_socket_addr(addr)?,
            Instant::now() + timeout
        ).await
    }

    #[inline(always)]
    async fn send_all_to<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A) -> Result<usize> {
        let mut sent = 0;
        let addr = sock_addr_from_to_socket_addr(addr)?;
        
        while sent < buf.len() {
            sent += SendTo::new(
                self.as_raw_fd(),
                buf,
                &addr
            ).await?;
        }
        
        Ok(sent)
    }

    #[inline(always)]
    async fn send_all_to_with_deadline<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A, deadline: Instant) -> Result<usize> {
        let mut sent = 0;
        let addr = sock_addr_from_to_socket_addr(addr)?;
        
        while sent < buf.len() {
            sent += SendToWithDeadline::new(
                self.as_raw_fd(),
                buf,
                &addr,
                deadline
            ).await?;
        }
        
        Ok(sent)
    }

    #[inline(always)]
    async fn send_all_to_with_timeout<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A, timeout: Duration) -> Result<usize> {
        self.send_all_to_with_deadline(buf, addr, Instant::now() + timeout).await
    }
}
use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::{mem};
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use nix::libc;
use socket2::SockAddr;

use crate::io::io_request::{IoRequest};
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{RawFd, FromRawFd, AsRawFd};
use crate::io::worker::{IoWorker, local_worker};
use crate::messages::BUG;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Accept<S: FromRawFd> {
    fd: RawFd,
    addr: (SockAddr, libc::socklen_t),
    io_request: Option<IoRequest>,
    pin: PhantomData<S>
}

impl<S: FromRawFd> Accept<S> {
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            addr: (unsafe { mem::zeroed() }, mem::size_of::<SockAddr>() as _),
            io_request: None,
            pin: PhantomData
        }
    }
}

impl<S: FromRawFd> Future for Accept<S> {
    type Output = Result<(S, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.accept(this.fd, this.addr.0.as_ptr() as _, &mut this.addr.1, this.io_request.as_mut().unwrap_unchecked()),
            (S::from_raw_fd(ret as RawFd), this.addr.0.clone())
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct AcceptWithDeadline<S: FromRawFd> {
    fd: RawFd,
    addr: (SockAddr, libc::socklen_t),
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
    pin: PhantomData<S>
}

impl<S: FromRawFd> AcceptWithDeadline<S> {
    pub fn new(fd: RawFd, deadline: Instant) -> Self {
        Self {
            fd,
            addr: (unsafe { mem::zeroed() }, mem::size_of::<SockAddr>() as _),
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
            pin: PhantomData
        }
    }
}

impl<S: FromRawFd> Future for AcceptWithDeadline<S> {
    type Output = Result<(S, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.accept(this.fd, this.addr.0.as_ptr() as _, &mut this.addr.1, this.io_request.as_mut().unwrap_unchecked()),
            (S::from_raw_fd(ret as RawFd), this.addr.0.clone())
        ));
    }
}

pub trait AsyncAccept<S: FromRawFd>: AsRawFd {
    #[inline(always)]
    async fn accept(&mut self) -> Result<(S, SocketAddr)> {
        let (stream, sock_addr) = Accept::<S>::new(self.as_raw_fd()).await?;
        Ok((stream, sock_addr.as_socket().expect(BUG)))
    }

    #[inline(always)]
    async fn accept_with_deadline(
        &mut self,
        deadline: Instant
    ) -> Result<(S, SocketAddr)> {
        let (stream, sock_addr) = AcceptWithDeadline::<S>::new(self.as_raw_fd(), deadline).await?;
        Ok((stream, sock_addr.as_socket().expect(BUG)))
    }

    #[inline(always)]
    async fn accept_with_timeout(
        &mut self,
        timeout: Duration
    ) -> Result<(S, SocketAddr)> {
        self.accept_with_deadline(Instant::now() + timeout).await
    }
}

pub trait AsyncAcceptUnix<S: FromRawFd>: AsRawFd {
    #[inline(always)]
    async fn accept(&mut self) -> Result<(S, std::os::unix::net::SocketAddr)> {
        let (stream, addr) = Accept::<S>::new(self.as_raw_fd()).await?;
        Ok((stream, addr.as_unix().expect(BUG)))
    }

    #[inline(always)]
    async fn accept_with_deadline(
        &mut self,
        deadline: Instant
    ) -> Result<(S, std::os::unix::net::SocketAddr)> {
        let (stream, addr) = AcceptWithDeadline::<S>::new(self.as_raw_fd(), deadline).await?;
        Ok((stream, addr.as_unix().expect(BUG)))
    }

    #[inline(always)]
    async fn accept_with_timeout(
        &mut self,
        timeout: Duration
    ) -> Result<(S, std::os::unix::net::SocketAddr)> {
        self.accept_with_deadline(Instant::now() + timeout).await
    }
}
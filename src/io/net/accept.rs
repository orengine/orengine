use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::{io, mem};
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use nix::libc;
use socket2::SockAddr;
use crate::io::io_request::{IoRequest};
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsFd, Fd};
use crate::io::worker::{IoWorker, local_worker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Accept<S: From<Fd>> {
    fd: Fd,
    addr: (SockAddr, libc::socklen_t),
    io_request: Option<IoRequest>,
    pin: PhantomData<S>
}

impl<S: From<Fd>> Accept<S> {
    pub fn new(fd: Fd) -> Self {
        Self {
            fd,
            addr: (unsafe { mem::zeroed() }, mem::size_of::<SockAddr>() as _),
            io_request: None,
            pin: PhantomData
        }
    }
}

impl<S: From<Fd>> Future for Accept<S> {
    type Output = Result<(S, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.accept(this.fd, this.addr.0.as_ptr() as _, &mut this.addr.1, this.io_request.as_ref().unwrap_unchecked()),
            (S::from(ret as Fd), this.addr.0.clone())
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct AcceptWithDeadline<S: From<Fd>> {
    fd: Fd,
    addr: (SockAddr, libc::socklen_t),
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
    pin: PhantomData<S>
}

impl<S: From<Fd>> AcceptWithDeadline<S> {
    pub fn new(fd: Fd, deadline: Instant) -> Self {
        Self {
            fd,
            addr: (unsafe { mem::zeroed() }, mem::size_of::<SockAddr>() as _),
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
            pin: PhantomData
        }
    }
}

impl<S: From<Fd>> Future for AcceptWithDeadline<S> {
    type Output = Result<(S, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.accept(this.fd, this.addr.0.as_ptr() as _, &mut this.addr.1, this.io_request.as_ref().unwrap_unchecked()),
            (S::from(ret as Fd), this.addr.0.clone())
        ));
    }
}

#[macro_export]
macro_rules! generate_accept {
    ($stream_ty:ty) => {
        #[inline(always)]
        pub async fn accept(&mut self) -> std::io::Result<$stream_ty> {
            let (stream, _) = crate::io::Accept::<$stream_ty>::new(self.as_raw_fd()).await?;
            Ok(stream)
        }

        #[inline(always)]
        pub async fn accept_with_deadline(
            &mut self,
            deadline: std::time::Instant
        ) -> std::io::Result<$stream_ty> {
            let (stream, _) = crate::io::AcceptWithDeadline::<$stream_ty>::new(self.as_raw_fd(), deadline).await?;
            Ok(stream)
        }

        #[inline(always)]
        pub async fn accept_with_timeout(
            &mut self,
            timeout: std::time::Duration
        ) -> std::io::Result<$stream_ty> {
            self.accept_with_deadline(std::time::Instant::now() + timeout).await
        }
    };

    ($stream_ty:ty, $addr_ty:ty, $to_addr_fn:expr) => {
        #[inline(always)]
        pub async fn accept(&mut self) -> std::io::Result<($stream_ty, $addr_ty)> {
            let (stream, sock_addr) = crate::io::Accept::<$stream_ty>::new(self.as_raw_fd()).await?;
            Ok((stream, $to_addr_fn(&sock_addr).unwrap()))
        }

        #[inline(always)]
        pub async fn accept_with_deadline(
            &mut self,
            deadline: std::time::Instant
        ) -> std::io::Result<($stream_ty, $addr_ty)> {
            let (stream, sock_addr) = crate::io::AcceptWithDeadline::<$stream_ty>::new(self.as_raw_fd(), deadline).await?;
            Ok((stream, $to_addr_fn(&sock_addr).unwrap()))
        }

        #[inline(always)]
        pub async fn accept_with_timeout(
            &mut self,
            timeout: std::time::Duration
        ) -> std::io::Result<($stream_ty, $addr_ty)> {
            self.accept_with_deadline(std::time::Instant::now() + timeout).await
        }
    };
}
use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::mem;
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
    type Output = Result<(S, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.accept(this.fd, this.addr.0.as_ptr() as _, &mut this.addr.1, this.io_request.as_ref().unwrap_unchecked()),
            (S::from(ret as Fd), this.addr.0.as_socket().expect("invalid addr"))
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
    type Output = Result<(S, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.accept(this.fd, this.addr.0.as_ptr() as _, &mut this.addr.1, this.io_request.as_ref().unwrap_unchecked()),
            (S::from(ret as Fd), this.addr.0.as_socket().expect("invalid addr"))
        ));
    }
}

pub trait AsyncAccept<S: From<Fd>>: AsFd {
    fn accept(&mut self) -> Accept<S> {
        Accept::new(self.as_raw_fd())
    }

    fn accept_with_deadline(&mut self, deadline: Instant) -> AcceptWithDeadline<S> {
        AcceptWithDeadline::new(self.as_raw_fd(), deadline)
    }

    fn accept_with_timeout(&mut self, timeout: Duration) -> AcceptWithDeadline<S> {
        AcceptWithDeadline::new(self.as_raw_fd(), Instant::now() + timeout)
    }
}
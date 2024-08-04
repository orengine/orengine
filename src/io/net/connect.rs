use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::Fd;
use crate::io::worker::{local_worker, IoWorker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Connect<S: From<Fd>> {
    fd: Fd,
    addr: SockAddr,
    io_request: Option<IoRequest>,
    phantom_data: PhantomData<S>,
}

impl<S: From<Fd>> Connect<S> {
    pub fn new(fd: Fd, addr: SockAddr) -> Self {
        Self {
            fd,
            addr,
            io_request: None,
            phantom_data: PhantomData,
        }
    }
}

impl<S: From<Fd>> Future for Connect<S> {
    type Output = Result<S>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
            worker.connect(
                this.fd,
                this.addr.as_ptr(),
                this.addr.len(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            S::from(this.fd)
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct ConnectWithTimeout<S: From<Fd>> {
    fd: Fd,
    addr: SockAddr,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
    phantom_data: PhantomData<S>,
}

impl<S: From<Fd>> ConnectWithTimeout<S> {
    pub fn new(fd: Fd, addr: SockAddr, deadline: Instant) -> Self {
        Self {
            fd,
            addr,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
            phantom_data: PhantomData,
        }
    }
}

impl<S: From<Fd>> Future for ConnectWithTimeout<S> {
    type Output = Result<S>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_time_bounded_io_request!((
            worker.connect(
                this.fd,
                this.addr.as_ptr(),
                this.addr.len(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            S::from(this.fd)
        ));
    }
}

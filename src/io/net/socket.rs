use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use orengine_macros::poll_for_io_request;
use socket2::{Domain, Type};
use crate::io::io_request::IoRequest;
use crate::io::sys::RawFd;
use crate::io::worker::{IoWorker, local_worker};

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Socket{
    domain: Domain,
    socket_type: Type,
    io_request: Option<IoRequest>
}

impl Socket {
    pub fn new(domain: Domain, socket_type: Type) -> Self {
        Self {
            domain,
            socket_type,
            io_request: None
        }
    }
}

impl Future for Socket {
    type Output = std::io::Result<RawFd>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.socket(this.domain, this.socket_type, this.io_request.as_mut().unwrap_unchecked()),
            ret as RawFd
        ));
    }
}
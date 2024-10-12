use crate::io::io_request_data::IoRequestData;
use crate::io::sys::RawFd;
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use socket2::{Domain, Type};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `socket` io operation.
pub struct Socket {
    domain: Domain,
    socket_type: Type,
    io_request_data: Option<IoRequestData>,
}

impl Socket {
    /// Creates new `socket` io operation.
    pub fn new(domain: Domain, socket_type: Type) -> Self {
        Self {
            domain,
            socket_type,
            io_request_data: None,
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
            worker.socket(
                this.domain,
                this.socket_type,
                this.io_request_data.as_mut().unwrap_unchecked()
            ),
            ret as RawFd
        ));
    }
}

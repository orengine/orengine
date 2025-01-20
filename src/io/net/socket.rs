use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::RawSocket;
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use socket2::{Domain, Protocol, Type};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `socket` io operation.
#[repr(C)]
pub struct Socket {
    domain: Domain,
    socket_type: Type,
    protocol: Protocol,
    io_request_data: Option<IoRequestData>,
}

impl Socket {
    /// Creates new `socket` io operation.
    pub fn new(domain: Domain, socket_type: Type, protocol: Protocol) -> Self {
        Self {
            domain,
            socket_type,
            protocol,
            io_request_data: None,
        }
    }
}

impl Future for Socket {
    type Output = std::io::Result<RawSocket>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        let ret;

        poll_for_io_request!((
            local_worker().socket(this.domain, this.socket_type, this.protocol, unsafe {
                IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked())
            }),
            ret as RawSocket
        ));
    }
}

unsafe impl Send for Socket {}

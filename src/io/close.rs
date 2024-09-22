use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use orengine_macros::poll_for_io_request;
use std::io::Result;
use crate::io::io_request_data::{IoRequestData};
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{IoWorker, local_worker};

/// `close` io operation.
#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Close {
    fd: RawFd,
    io_request_data: Option<IoRequestData>
}

impl Close {
    /// Create a new `Close` future.
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            io_request_data: None
        }
    }
}

impl Future for Close {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
             worker.close(this.fd, this.io_request_data.as_mut().unwrap_unchecked()),
             ()
        ));
    }
}

/// The [`AsyncClose`] trait represents an asynchronous close operation.
///
/// This trait can be implemented for all types that implement [`AsRawFd`].
pub trait AsyncClose: AsRawFd {
    /// Returns future that closes the file descriptor.
    ///
    /// # Be careful
    ///
    /// Some structs (like all structs in [`orengine::net`](crate::net)
    /// and [`orengine::fs`](crate::fs)) implements [`Drop`](Drop) that calls [`close`](Self::close).
    ///
    /// So, before call [`close`](Self::close) you should check if the struct implements auto-closing.
    fn close(&mut self) -> Close {
        Close::new(self.as_raw_fd())
    }
}
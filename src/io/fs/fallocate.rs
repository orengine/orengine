use std::future::Future;
use std::pin::Pin;
use std::io::Result;
use std::task::{Context, Poll};
use orengine_macros::{poll_for_io_request};

use crate::io::sys::{AsRawFd, RawFd};
use crate::io::io_request_data::{IoRequestData};
use crate::io::worker::{IoWorker, local_worker};

/// `fallocate` io operation which allows to allocate space in a file from a given offset.
#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Fallocate {
    fd: RawFd,
    offset: usize,
    len: usize,
    flags: i32,
    io_request_data: Option<IoRequestData>
}

impl Fallocate {
    /// Creates a new `fallocate` io operation.
    pub fn new(fd: RawFd, offset: usize, len: usize, flags: i32) -> Self {
        Self {
            fd,
            len,
            offset,
            flags,
            io_request_data: None
        }
    }
}

impl Future for Fallocate {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.fallocate(
                this.fd,
                this.offset as u64,
                this.len as u64,
                this.flags,
                this.io_request_data.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

/// This trait allows to create a `fallocate` io operation
/// which allows to allocate space in a file from a given offset.
///
/// Call [`fallocate`](AsyncFallocate::fallocate) to allocate len bytes on the disk.
pub trait AsyncFallocate: AsRawFd {
    #[inline(always)]
    fn fallocate(&self, offset: usize, len: usize, flags: i32) -> Fallocate {
        Fallocate::new(self.as_raw_fd(), offset, len, flags)
    }
}
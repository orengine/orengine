use std::future::Future;
use std::pin::Pin;
use std::io::Result;
use std::task::{Context, Poll};
use io_macros::{poll_for_io_request};

use crate::io::sys::{AsRawFd, RawFd};
use crate::io::io_request::{IoRequest};
use crate::io::worker::{IoWorker, local_worker};

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Fallocate {
    fd: RawFd,
    len: usize,
    io_request: Option<IoRequest>
}

impl Fallocate {
    pub fn new(fd: RawFd, len: usize) -> Self {
        Self {
            fd,
            len,
            io_request: None
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
             worker.fallocate(this.fd, this.len as u64, this.io_request.as_mut().unwrap_unchecked()),
             ret
        ));
    }
}

pub trait AsyncFallocate: AsRawFd {
    #[inline(always)]
    fn fallocate(&self, len: usize) -> Fallocate {
        Fallocate::new(self.as_raw_fd(), len)
    }
}
use std::future::Future;
use std::pin::Pin;
use std::io::Result;
use std::task::{Context, Poll};
use io_macros::{poll_for_io_request};

use crate::io::sys::{AsRawFd, RawFd};
use crate::io::io_request::{IoRequest};
use crate::io::worker::{IoWorker, local_worker};

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct SyncAll {
    fd: RawFd,
    io_request: Option<IoRequest>
}

impl SyncAll {
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            io_request: None
        }
    }
}

impl Future for SyncAll {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
             worker.sync_all(this.fd, this.io_request.as_mut().unwrap_unchecked()),
             ret
        ));
    }
}

pub trait AsyncSyncAll: AsRawFd {
    #[inline(always)]
    fn sync_all(&self) -> SyncAll {
        SyncAll::new(self.as_raw_fd())
    }
}
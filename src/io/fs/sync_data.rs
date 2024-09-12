use std::future::Future;
use std::pin::Pin;
use std::io::Result;
use std::task::{Context, Poll};
use io_macros::{poll_for_io_request};

use crate::io::sys::{AsRawFd, RawFd};
use crate::io::io_request::{IoRequest};
use crate::io::worker::{IoWorker, local_worker};

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct SyncData {
    fd: RawFd,
    io_request: Option<IoRequest>
}

impl SyncData {
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            io_request: None
        }
    }
}

impl Future for SyncData {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
             worker.sync_data(this.fd, this.io_request.as_mut().unwrap_unchecked()),
             ret
        ));
    }
}

pub trait AsyncSyncData: AsRawFd {
    #[inline(always)]
    fn sync_data(&self) -> SyncData {
        SyncData::new(self.as_raw_fd())
    }
}
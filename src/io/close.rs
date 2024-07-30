use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use io_macros::{poll_for_io_request};
use std::io::Result;
use crate::io::io_request::{IoRequest};
use crate::io::sys::{AsFd, Fd};
use crate::io::worker::{IoWorker, local_worker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub(crate) struct Close {
    fd: Fd,
    io_request: Option<IoRequest>
}

impl Close {
    pub(crate) fn new(fd: Fd) -> Self {
        Self {
            fd,
            io_request: None
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
             worker.close(this.fd, this.io_request.as_ref().unwrap_unchecked()),
             ()
        ));
    }
}

pub(crate) trait AsyncClose: AsFd {
    fn close(&mut self) -> Close {
        Close::new(self.as_raw_fd())
    }
}
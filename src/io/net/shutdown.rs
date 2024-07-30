use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use io_macros::{poll_for_io_request};
use std::io::Result;
use std::net::Shutdown as ShutdownHow;
use crate::io::io_request::{IoRequest};
use crate::io::sys::{AsFd, Fd};
use crate::io::worker::{IoWorker, local_worker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Shutdown {
    fd: Fd,
    how: ShutdownHow,
    io_request: Option<IoRequest>
}

impl Shutdown {
    pub fn new(fd: Fd, how: ShutdownHow) -> Self {
        Self {
            fd,
            how,
            io_request: None
        }
    }
}

impl Future for Shutdown {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
             worker.shutdown(this.fd, this.how, this.io_request.as_ref().unwrap_unchecked()),
             ()
        ));
    }
}

pub trait AsyncShutdown: AsFd {
    fn shutdown(&mut self, how: ShutdownHow) -> Shutdown {
        Shutdown::new(self.as_raw_fd(), how)
    }
}
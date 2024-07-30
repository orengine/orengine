use std::future::Future;
use std::pin::Pin;
use std::io::{Result};
use std::marker::PhantomData;
use std::task::{Context, Poll};
use io_macros::poll_for_io_request;
use crate::io::io_request::{IoRequest};
use crate::io::sys::Fd;
use crate::io::sys::OsPath::OsPath;
use crate::io::sys::unix::OsOpenOptions;
use crate::io::worker::{IoWorker, local_worker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Open<F: From<Fd>> {
    path: OsPath,
    os_open_options: OsOpenOptions,
    io_request: Option<IoRequest>,
    phantom_data: PhantomData<F>
}

impl<F: From<Fd>> Open<F> {
    pub fn new(path: OsPath, os_open_options: OsOpenOptions) -> Self {
        Self {
            path,
            os_open_options,
            io_request: None,
            phantom_data: PhantomData
        }
    }
}

impl<F: From<Fd>> Future for Open<F> {
    type Output = Result<F>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.open(this.path.as_ptr(), &this.os_open_options, this.io_request.as_ref().unwrap_unchecked()),
            F::from(ret as Fd)
        ));
    }
}
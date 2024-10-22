use crate::io::io_request_data::IoRequestData;
use crate::io::sys::unix::OsOpenOptions;
use crate::io::sys::OsPath::OsPath;
use crate::io::sys::{FromRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `open` io operation which opens a file at the given path with the given options.
pub struct Open<F: FromRawFd> {
    path: OsPath,
    os_open_options: OsOpenOptions,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<F>,
}

impl<F: FromRawFd> Open<F> {
    /// Creates a new `open` io operation.
    pub fn new(path: OsPath, os_open_options: OsOpenOptions) -> Self {
        Self {
            path,
            os_open_options,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl<F: FromRawFd> Future for Open<F> {
    type Output = Result<F>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_io_request!((
            worker.open(this.path.as_ptr(), &this.os_open_options, unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            unsafe { F::from_raw_fd(ret as RawFd) }
        ));
    }
}

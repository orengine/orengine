use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{get_os_path_ptr, OsPath};
use crate::io::sys::{FromRawFile, OsOpenOptions, RawFile};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `open` io operation which opens a file at the given path with the given options.
#[repr(C)]
pub struct Open<F: FromRawFile> {
    path: OsPath,
    os_open_options: OsOpenOptions,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<F>,
}

impl<F: FromRawFile> Open<F> {
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

impl<F: FromRawFile> Future for Open<F> {
    type Output = Result<F>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().open(get_os_path_ptr(&this.path), &this.os_open_options, unsafe {
                IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked())
            }),
            unsafe { F::from_raw_file(ret as RawFile) }
        ));
    }
}

unsafe impl<F: FromRawFile> Send for Open<F> {}

use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{get_os_path_ptr, OsPath};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Create a directory at the given path.
#[repr(C)]
pub struct CreateDir {
    mode: u32,
    os_path: OsPath,
    io_request_data: Option<IoRequestData>,
}

impl CreateDir {
    /// Creates a new `CreateDir` future for the given path.
    pub fn new(path: OsPath, mode: u32) -> Self {
        Self {
            mode,
            os_path: path,
            io_request_data: None,
        }
    }
}

unsafe impl Send for CreateDir {}

impl Future for CreateDir {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().create_dir(get_os_path_ptr(&this.os_path), this.mode, unsafe {
                IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked())
            }),
            ()
        ));
    }
}

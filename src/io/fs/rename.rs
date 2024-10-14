use crate::io::io_request_data::IoRequestData;
use crate::io::sys::OsPath::OsPath;
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Rename a file or directory from one path to another.
pub struct Rename {
    old_path: OsPath,
    new_path: OsPath,
    io_request_data: Option<IoRequestData>,
}

impl Rename {
    /// Creates a new `rename` io operation.
    pub fn new(old_path: OsPath, new_path: OsPath) -> Self {
        Self {
            old_path,
            new_path,
            io_request_data: None,
        }
    }
}

impl Future for Rename {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
            worker.rename(this.old_path.as_ptr(), this.new_path.as_ptr(), unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ()
        ));
    }
}

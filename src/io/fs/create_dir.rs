use std::future::Future;
use std::io::{Result};
use std::pin::Pin;
use std::task::{Context, Poll};
use orengine_macros::poll_for_io_request;
use crate::io::io_request::{IoRequest};
use crate::io::sys::OsPath::OsPath;
use crate::io::worker::{IoWorker, local_worker};

/// Create a directory at the given path.
#[must_use = "Future must be awaited to drive the IO operation"]
pub struct CreateDir {
    mode: u32,
    os_path: OsPath,
    io_request: Option<IoRequest>
}

impl CreateDir {
    /// Creates a new `CreateDir` future for the given path.
    pub fn new(path: OsPath, mode: u32) -> Self {
        Self {
            mode,
            os_path: path,
            io_request: None
        }
    }
}

impl Future for CreateDir {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
            worker.create_dir(
                this.os_path.as_ptr(),
                this.mode,
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ()
        ));
    }
}
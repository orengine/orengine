use std::future::Future;
use std::io::{Result};
use std::pin::Pin;
use std::task::{Context, Poll};
use orengine_macros::poll_for_io_request;
use crate::io::io_request_data::{IoRequestData};
use crate::io::sys::OsPath::OsPath;
use crate::io::worker::{IoWorker, local_worker};

/// `remove_dir` io operation from a given path.
#[must_use = "Future must be awaited to drive the IO operation"]
pub struct RemoveDir {
    path: OsPath,
    io_request_data: Option<IoRequestData>
}

impl RemoveDir {
    /// Creates a new 'remove_dir' io operation from a given path.
    pub fn new(path: OsPath) -> Self {
        Self {
            path,
            io_request_data: None
        }
    }
}

impl Future for RemoveDir {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
            worker.remove_dir(this.path.as_ptr(), this.io_request_data.as_mut().unwrap_unchecked()),
            ()
        ));
    }
}
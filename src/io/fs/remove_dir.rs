use crate::io::io_request_data::IoRequestData;
use crate::io::sys::OsPath::OsPath;
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `remove_dir` io operation from a given path.
pub struct RemoveDir {
    path: OsPath,
    io_request_data: Option<IoRequestData>,
}

impl RemoveDir {
    /// Creates a new 'remove_dir' io operation from a given path.
    pub fn new(path: OsPath) -> Self {
        Self {
            path,
            io_request_data: None,
        }
    }
}

impl Future for RemoveDir {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
            worker.remove_dir(this.path.as_ptr(), unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ()
        ));
    }
}

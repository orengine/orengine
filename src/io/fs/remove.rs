use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{get_os_path_ptr, OsPath};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `remove` io operation which allows to remove a file from a given path.
pub struct Remove {
    path: OsPath,
    io_request_data: Option<IoRequestData>,
}

impl Remove {
    /// Creates a new `remove` io operation.
    pub fn new(path: OsPath) -> Self {
        Self {
            path,
            io_request_data: None,
        }
    }
}

impl Future for Remove {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().remove_file(get_os_path_ptr(&this.path), unsafe {
                IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked())
            }),
            ()
        ));
    }
}

unsafe impl Send for Remove {}

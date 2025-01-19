use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{get_os_path_ptr, OsPath};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `remove_dir` io operation from a given path.
#[repr(C)]
pub struct RemoveDir {
    path: OsPath,
    io_request_data: Option<IoRequestData>,
}

impl RemoveDir {
    /// Creates a new `remove_dir` io operation from a given path.
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
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().remove_dir(get_os_path_ptr(&this.path), unsafe {
                IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked())
            }),
            ()
        ));
    }
}

unsafe impl Send for RemoveDir {}

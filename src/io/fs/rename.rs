use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{get_os_path_ptr, OsPath};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Rename a file or directory from one path to another.
#[repr(C)]
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

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().rename(
                get_os_path_ptr(&this.old_path),
                get_os_path_ptr(&this.new_path),
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ()
        ));
    }
}

unsafe impl Send for Rename {}

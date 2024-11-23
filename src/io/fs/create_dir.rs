use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::OsPath::OsPath;
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Create a directory at the given path.
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

impl Future for CreateDir {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
            local_worker().create_dir(this.os_path.as_ptr(), this.mode, unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ()
        ));
    }
}

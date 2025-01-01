use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFile, RawFile};
use crate::io::worker::{local_worker, IoWorker};

/// `fallocate` io operation which allows to allocate space in a file from a given offset.
pub struct Fallocate {
    raw_file: RawFile,
    offset: usize,
    len: usize,
    flags: i32,
    io_request_data: Option<IoRequestData>,
}

impl Fallocate {
    /// Creates a new `fallocate` io operation.
    pub fn new(raw_file: RawFile, offset: usize, len: usize, flags: i32) -> Self {
        Self {
            raw_file,
            len,
            offset,
            flags,
            io_request_data: None,
        }
    }
}

impl Future for Fallocate {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().fallocate(
                this.raw_file,
                this.offset as u64,
                this.len as u64,
                this.flags,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

unsafe impl Send for Fallocate {}

/// This trait allows to create a `fallocate` io operation
/// which allows to allocate space in a file from a given offset.
///
/// Call [`fallocate`](AsyncFallocate::fallocate) to allocate len bytes on the disk.
pub trait AsyncFallocate: AsRawFile {
    #[inline(always)]
    fn fallocate(
        &self,
        offset: usize,
        len: usize,
        flags: i32,
    ) -> impl Future<Output = Result<usize>> {
        Fallocate::new(self.as_raw_file(), offset, len, flags)
    }
}

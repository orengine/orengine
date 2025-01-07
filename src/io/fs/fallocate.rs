use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
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
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().fallocate(
                this.raw_file,
                this.offset as u64,
                this.len as u64,
                this.flags,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ()
        ));
    }
}

unsafe impl Send for Fallocate {}

/// This trait allows to create a `fallocate` io operation
/// which allows to allocate space in a file from a given offset.
///
/// Call [`fallocate`](AsyncFallocate::fallocate) to allocate len bytes on the disk.
pub trait AsyncFallocate: AsRawFile {
    /// Allocate space in a file from a given offset.
    ///
    /// The manipulated range starts at the `offset` and continues for `len` bytes.
    ///
    /// The specific actions with the allocated disk space are specified by
    /// the `flags`, read `fallocate(2)` man page for more details.
    ///
    /// Not all OS and not all file systems support this operation. If the operation is not supported,
    /// it does nothing and returns `Ok(())`.  
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncFallocate;
    ///
    /// async fn foo() {
    /// let f = File::open("foo.txt", &OpenOptions::new().write(true).create(true)).await.unwrap();
    ///
    /// // Allocate a 1024 byte file without filling it, like Vec::reserve
    /// #[cfg(target_os = "linux")]
    /// f.fallocate(0, 1024, libc::FALLOC_FL_KEEP_SIZE).await.unwrap();
    /// # }
    /// ```
    #[inline(always)]
    fn fallocate(&self, offset: usize, len: usize, flags: i32) -> impl Future<Output = Result<()>> {
        Fallocate::new(self.as_raw_file(), offset, len, flags)
    }
}

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFile, RawFile};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `sync_data` io operation.
pub struct SyncData {
    raw_file: RawFile,
    io_request_data: Option<IoRequestData>,
}

impl SyncData {
    /// Creates a new `sync_data` io operation.
    pub fn new(raw_file: RawFile) -> Self {
        Self {
            raw_file,
            io_request_data: None,
        }
    }
}

impl Future for SyncData {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().sync_data(this.raw_file, unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ret
        ));
    }
}

unsafe impl Send for SyncData {}

/// The [`AsyncSyncData`] trait provides
/// a [`sync_data`](AsyncSyncData::sync_data) method to synchronize the data of a
/// file with the underlying storage device.
///
/// For more details, see [`sync_data`](AsyncSyncData::sync_data).
pub trait AsyncSyncData: AsRawFile {
    /// This function is similar to [`sync_all`](crate::io::AsyncSyncAll::sync_all),
    /// except that it might not synchronize file metadata to the filesystem.
    ///
    /// This is intended for use cases that must synchronize content, but don't
    /// need the metadata on disk. The goal of this method is to reduce disk operations.
    ///
    /// Note that some platforms may simply implement
    /// this in terms of [`sync_all`](crate::io::AsyncSyncAll::sync_all).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::{AsyncRead, AsyncSyncData, AsyncWrite, buffer};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    ///
    /// let mut buf = buffer();
    /// buf.append(b"Hello, world!");
    ///
    /// file.write_all(&buf).await?;
    /// file.sync_data().await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn sync_data(&self) -> impl Future<Output = Result<usize>> {
        SyncData::new(self.as_raw_file())
    }
}

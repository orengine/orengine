use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};

/// `write` io operation.
pub struct Write<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> Write<'buf> {
    /// Creates a new `write` io operation.
    pub fn new(fd: RawFd, buf: &'buf [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request_data: None,
        }
    }
}

impl<'buf> Future for Write<'buf> {
    type Output = Result<usize>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().write(this.fd, this.buf.as_ptr(), this.buf.len(), unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ret
        ));
    }
}

/// `pwrite` io operation.
pub struct PositionedWrite<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    offset: usize,
    io_request_data: Option<IoRequestData>,
}

impl<'buf> PositionedWrite<'buf> {
    /// Creates a new `pwrite` io operation.
    pub fn new(fd: RawFd, buf: &'buf [u8], offset: usize) -> Self {
        Self {
            fd,
            buf,
            offset,
            io_request_data: None,
        }
    }
}

impl<'buf> Future for PositionedWrite<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().pwrite(
                this.fd,
                this.buf.as_ptr(),
                this.buf.len(),
                this.offset,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

/// The `AsyncWrite` trait provides asynchronous methods for writing to a file descriptor.
///
/// This trait is implemented for types that can be represented
/// as raw file descriptors (via [`AsRawFd`]). It includes basic asynchronous write operations,
/// as well as methods for performing positioned writes.
///
/// # Example
///
/// ```rust
/// use orengine::fs::File;
/// use orengine::fs::OpenOptions;
/// use orengine::io::AsyncWrite;
///
/// # async fn foo() -> std::io::Result<()> {
/// let options = OpenOptions::new().write(true);
/// let mut file = File::open("example.txt", &options).await?;
///
/// // Asynchronously write to the file
/// file.write_all(b"Hello, world!").await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncWrite: AsRawFd {
    /// Asynchronously writes data from the provided buffer to the file descriptor.
    ///
    /// This method write some bytes from the buffer to the file descriptor.
    /// It returns a future that resolves to the number of bytes written.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::File;
    /// use orengine::fs::OpenOptions;
    /// use orengine::io::AsyncWrite;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let bytes_written = file.write(b"Hello, world!").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn write<'buf>(&mut self, buf: &'buf [u8]) -> Write<'buf> {
        Write::new(self.as_raw_fd(), buf)
    }

    /// Asynchronously performs a positioned write, writing to the file at the specified offset.
    ///
    /// This method does not modify the file's current position but instead writes to the specified
    /// `offset`. It returns a future that resolves to the number of bytes written.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::File;
    /// use orengine::fs::OpenOptions;
    /// use orengine::buf::full_buffer;
    /// use orengine::io::AsyncWrite;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let bytes_written = file.pwrite(b"Hello, world!", 1024).await?;  // Write at offset 1024
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn pwrite<'buf>(&mut self, buf: &'buf [u8], offset: usize) -> PositionedWrite<'buf> {
        PositionedWrite::new(self.as_raw_fd(), buf, offset)
    }

    /// Asynchronously writes all data from the buffer to the file descriptor.
    ///
    /// This method continues writing until the entire buffer is written to the file descriptor.
    /// It will return an error if the write operation fails or cannot complete.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::File;
    /// use orengine::fs::OpenOptions;
    /// use orengine::io::AsyncWrite;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// file.write_all(b"Hello, world").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        let mut written = 0;

        while written < buf.len() {
            written += self.write(&buf[written..]).await?;
        }
        Ok(())
    }

    /// Asynchronously performs a positioned write, writing all data from the buffer starting at the specified offset.
    ///
    /// This method continues writing from the specified `offset` until the entire buffer is written.
    /// If the write operation fails or cannot complete, it will return an error.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::File;
    /// use orengine::fs::OpenOptions;
    /// use orengine::io::AsyncWrite;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// file.pwrite_all(b"Hello, world", 512).await?;  // Write all starting at offset 512
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn pwrite_all(&mut self, buf: &[u8], offset: usize) -> Result<()> {
        let mut written = 0;
        while written < buf.len() {
            written += self.pwrite(&buf[written..], offset + written).await?;
        }
        Ok(())
    }
}

use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future for the `read` operation.
pub struct Read<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> Read<'buf> {
    /// Creates a new `read` io operation.
    pub fn new(fd: RawFd, buf: &'buf mut [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request_data: None,
        }
    }
}

impl<'buf> Future for Read<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.read(this.fd, this.buf.as_mut_ptr(), this.buf.len(), unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ret
        ));
    }
}

/// Future for the `pread` operation.
///
/// # Positional read
///
/// This is a variation of `read` that allows
/// to specify the offset from which the data should be read.
pub struct PositionedRead<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    offset: usize,
    io_request_data: Option<IoRequestData>,
}

impl<'buf> PositionedRead<'buf> {
    /// Creates a new `pread` io operation.
    pub fn new(fd: RawFd, buf: &'buf mut [u8], offset: usize) -> Self {
        Self {
            fd,
            buf,
            offset,
            io_request_data: None,
        }
    }
}

impl<'buf> Future for PositionedRead<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.pread(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len(),
                this.offset,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

/// The `AsyncRead` trait provides asynchronous methods for reading bytes from readers.
///
/// This trait is implemented for types that can be represented
/// as raw file descriptors (via [`AsRawFd`]).
///
/// It includes basic asynchronous read operations,
/// as well as methods for performing positioned reads.
///
/// # Example
///
/// ```no_run
/// use std::ops::Deref;
/// use orengine::buf::full_buffer;
/// use orengine::fs::{File, OpenOptions};
/// use orengine::io::{AsyncRead, AsyncWrite};
///
/// # async fn foo() -> std::io::Result<()> {
/// let options = OpenOptions::new()
///                 .read(true)
///                 .write(true)
///                 .create(true);
/// let mut file = File::open("example.txt", &options).await?;
/// file.write_all(b"Hello world!").await?;
/// let mut buffer = full_buffer();
///
/// // Asynchronously read into buffer
/// let bytes_read = file.read(&mut buffer).await?;
/// // Asynchronously read exactly 12 bytes
/// file.read_exact(&mut buffer[..12]).await?;
/// assert_eq!(&buffer[..12], b"Hello world!");
///
/// let bytes_read = file.pread(&mut buffer, 6).await?;
/// // or read exactly 6 bytes
/// file.pread_exact(&mut buffer, 6).await?;
/// assert_eq!(&buffer[..6], b"world!");
/// # Ok(())
/// # }
/// ```
pub trait AsyncRead: AsRawFd {
    /// Asynchronously reads data from the reader into the provided buffer.
    ///
    /// This method starts reading from the current file position
    /// and reads up to the length of the buffer.
    /// It returns a future that resolves to the number of bytes read.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = full_buffer();
    /// let bytes_read = file.read(buffer.as_mut()).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn read<'buf>(&mut self, buf: &'buf mut [u8]) -> Read<'buf> {
        Read::new(self.as_raw_fd(), buf)
    }

    /// Asynchronously performs a positioned read, reading from the file at the specified offset.
    ///
    /// This method does not modify the file's current position but instead reads from the specified
    /// `offset`. It returns a future that resolves to the number of bytes read.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = full_buffer();
    /// let bytes_read = file.pread(buffer.as_mut(), 1024).await?;  // Read starting from offset 1024
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn pread<'buf>(&mut self, buf: &'buf mut [u8], offset: usize) -> PositionedRead<'buf> {
        PositionedRead::new(self.as_raw_fd(), buf, offset)
    }

    /// Asynchronously reads the exact number of bytes required to fill the buffer.
    ///
    /// This method continuously reads from the file descriptor until the entire buffer is filled.
    /// If the end of the file is reached before filling the buffer, it returns an error.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = full_buffer();
    /// file.read_exact(buffer.as_mut()).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut read = 0;

        while read < buf.len() {
            read += self.read(&mut buf[read..]).await?;
        }

        Ok(())
    }

    /// Asynchronously performs a positioned read, reading exactly the number of bytes needed to fill the buffer.
    ///
    /// This method reads data starting at the specified `offset` until the entire buffer is filled.
    /// If the end of the file is reached before filling the buffer, it returns an error.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = full_buffer();
    /// file.pread_exact(buffer.as_mut(), 512).await?;  // Read exactly starting from offset 512
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn pread_exact(&mut self, buf: &mut [u8], offset: usize) -> Result<()> {
        let mut read = 0;

        while read < buf.len() {
            read += self.pread(&mut buf[read..], offset + read).await?;
        }

        Ok(())
    }
}

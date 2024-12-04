use crate as orengine;
use crate::buf::Buffer;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
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

impl Future for Read<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().recv(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

/// Future for the `read` operation with [`Buffer`].
pub struct ReadFixed<'buf> {
    fd: RawFd,
    ptr: *mut u8,
    len: u32,
    buf_index: u16,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'buf ()>,
}

impl<'buf> ReadFixed<'buf> {
    /// Creates a new `read` io operation.
    pub fn new(fd: RawFd, ptr: *mut u8, len: u32, buf_index: u16) -> Self {
        Self {
            fd,
            ptr,
            len,
            buf_index,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl Future for ReadFixed<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().recv_fixed(this.fd, this.ptr, this.len, this.buf_index, unsafe {
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

impl Future for PositionedRead<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().pread(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                this.offset,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

/// Future for the `pread` operation with [`Buffer`].
///
/// # Positional read
///
/// This is a variation of `read` that allows
/// to specify the offset from which the data should be read.
pub struct PositionedReadFixed<'buf> {
    fd: RawFd,
    ptr: *mut u8,
    len: u32,
    buf_index: u16,
    offset: usize,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'buf ()>,
}

impl<'buf> PositionedReadFixed<'buf> {
    /// Creates a new `pread` io operation.
    pub fn new(fd: RawFd, ptr: *mut u8, len: u32, buf_index: u16, offset: usize) -> Self {
        Self {
            fd,
            ptr,
            len,
            buf_index,
            offset,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl Future for PositionedReadFixed<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().pread_fixed(
                this.fd,
                this.ptr,
                this.len,
                this.buf_index,
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
/// ```rust
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
/// buffer.set_len(12).expect("12 > buffer.capacity()");
/// // Asynchronously read exactly 12 bytes
/// file.read_exact(&mut buffer).await?;
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
    /// Asynchronously reads data from the reader into the provided byte array.
    ///
    /// This method starts reading from the current file position
    /// and reads up to the length of the buffer.
    /// It returns a future that resolves to the number of bytes read.
    ///
    /// # Difference between `read` and `read_bytes`
    ///
    /// Use [`read`](Self::read) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut arr = vec![0; 1024];
    /// let bytes_read = file.read_bytes(arr.as_mut()).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn read_bytes<'buf>(&mut self, buf: &'buf mut [u8]) -> Read<'buf> {
        Read::new(self.as_raw_fd(), buf)
    }

    /// Asynchronously reads data from the reader into the provided [`Buffer`].
    ///
    /// This method starts reading from the current file position
    /// and reads up to the length of the buffer.
    /// It returns a future that resolves to the number of bytes read.
    ///
    /// # Difference between `read` and `read_bytes`
    ///
    /// Use [`read`](Self::read) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = full_buffer();
    /// let bytes_read = file.read(&mut buffer).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn read<'buf>(&mut self, buf: &'buf mut Buffer) -> ReadFixed<'buf> {
        ReadFixed::new(
            self.as_raw_fd(),
            buf.as_mut_ptr(),
            buf.len_u32(),
            buf.fixed_index(),
        )
    }

    /// Asynchronously performs a positioned read, reading from the file at the specified offset.
    ///
    /// This method does not modify the file's current position but instead reads from the specified
    /// `offset`. It returns a future that resolves to the number of bytes read.
    ///
    /// # Difference between `pread` and `pread_bytes`
    ///
    /// Use [`pread`](Self::pread) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut arr = vec![0; 1024];
    /// let bytes_read = file.pread_bytes(arr.as_mut(), 1024).await?;  // Read starting from offset 1024
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn pread_bytes<'buf>(&mut self, buf: &'buf mut [u8], offset: usize) -> PositionedRead<'buf> {
        PositionedRead::new(self.as_raw_fd(), buf, offset)
    }

    /// Asynchronously performs a positioned read, reading from the file at the specified offset.
    ///
    /// This method does not modify the file's current position but instead reads from the specified
    /// `offset`. It returns a future that resolves to the number of bytes read.
    ///
    /// # Difference between `pread` and `pread_bytes`
    ///
    /// Use [`pread`](Self::pread) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buf = full_buffer();
    /// let bytes_read = file.pread(&mut buf, 1024).await?;  // Read starting from offset 1024
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn pread(&mut self, buf: &mut Buffer, offset: usize) -> Result<usize> {
        if buf.is_fixed() {
            PositionedReadFixed::new(
                self.as_raw_fd(),
                buf.as_mut_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
                offset,
            )
            .await
        } else {
            PositionedRead::new(self.as_raw_fd(), buf, offset).await
        }
    }

    /// Asynchronously reads the exact number of bytes required to fill the byte array.
    ///
    /// This method continuously reads from the file descriptor until the entire buffer is filled.
    /// If the end of the file is reached before filling the buffer, it returns an error.
    ///
    /// # Difference between `read_exact` and `read_bytes_exact`
    ///
    /// Use [`read_exact`](Self::read_exact) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut arr = vec![0; 1024];
    /// file.read_bytes_exact(arr.as_mut()).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn read_bytes_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut read = 0;

        while read < buf.len() {
            read += self.read_bytes(&mut buf[read..]).await?;
        }

        Ok(())
    }

    /// Asynchronously reads the exact number of bytes required to fill the [`Buffer`].
    ///
    /// This method continuously reads from the file descriptor until the entire buffer is filled.
    /// If the end of the file is reached before filling the buffer, it returns an error.
    ///
    /// # Difference between `read_exact` and `read_bytes_exact`
    ///
    /// Use [`read_exact`](Self::read_exact) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = full_buffer();
    /// file.read_exact(&mut buffer).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn read_exact(&mut self, buf: &mut Buffer) -> Result<()> {
        let mut read = 0;

        if buf.is_fixed() {
            while read < buf.len() {
                read += ReadFixed::new(
                    self.as_raw_fd(),
                    unsafe { buf.as_mut_ptr().offset(read as isize) },
                    buf.len_u32(),
                    buf.fixed_index(),
                )
                .await?;
            }
        } else {
            while read < buf.len() {
                read += self.read_bytes(&mut buf[read..]).await?;
            }
        }

        Ok(())
    }

    /// Asynchronously performs a positioned read, reading exactly the number of bytes needed to fill the byte array.
    ///
    /// This method reads data starting at the specified `offset` until the entire buffer is filled.
    /// If the end of the file is reached before filling the buffer, it returns an error.
    ///
    /// # Difference between `pread_exact` and `pread_bytes_exact`
    ///
    /// Use [`pread_exact`](Self::pread_exact) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut arr = vec![0; 1024];
    /// file.pread_bytes_exact(arr.as_mut(), 512).await?;  // Read exactly starting from offset 512
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn pread_bytes_exact(&mut self, buf: &mut [u8], offset: usize) -> Result<()> {
        let mut read = 0;

        while read < buf.len() {
            read += self.pread_bytes(&mut buf[read..], offset + read).await?;
        }

        Ok(())
    }

    /// Asynchronously performs a positioned read, reading exactly the number
    /// of bytes needed to fill the [`Buffer`].
    ///
    /// This method reads data starting at the specified `offset` until the entire buffer is filled.
    /// If the end of the file is reached before filling the buffer, it returns an error.
    ///
    /// # Difference between `pread_exact` and `pread_bytes_exact`
    ///
    /// Use [`pread_exact`](Self::pread_exact) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::buf::full_buffer;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::AsyncRead;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = full_buffer();
    /// file.pread_exact(&mut buffer, 512).await?;  // Read exactly starting from offset 512
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn pread_exact(&mut self, buf: &mut Buffer, offset: usize) -> Result<()> {
        let mut read = 0;

        if buf.is_fixed() {
            while read < buf.len() {
                read += PositionedReadFixed::new(
                    self.as_raw_fd(),
                    unsafe { buf.as_mut_ptr().offset(read as isize) },
                    buf.len_u32(),
                    buf.fixed_index(),
                    offset + read,
                )
                .await?;
            }
        } else {
            while read < buf.len() {
                read += self.pread_bytes(&mut buf[read..], offset + read).await?;
            }
        }

        Ok(())
    }
}

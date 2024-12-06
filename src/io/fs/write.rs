use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::{Buffer, FixedBuffer};

/// `write` io operation.
pub struct WriteBytes<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> WriteBytes<'buf> {
    /// Creates a new `write` io operation.
    pub fn new(fd: RawFd, buf: &'buf [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request_data: None,
        }
    }
}

impl Future for WriteBytes<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().write(this.fd, this.buf.as_ptr(), this.buf.len() as u32, unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ret
        ));
    }
}

/// `write` io operation with [`Buffer`].
pub struct WriteFixed<'buf> {
    fd: RawFd,
    ptr: *const u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'buf Buffer>,
}

impl<'buf> WriteFixed<'buf> {
    /// Creates a new `write` io operation.
    pub fn new(fd: RawFd, ptr: *const u8, len: u32, fixed_index: u16) -> Self {
        Self {
            fd,
            ptr,
            len,
            fixed_index,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl Future for WriteFixed<'_> {
    type Output = Result<u32>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().write_fixed(this.fd, this.ptr, this.len, this.fixed_index, unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ret as u32
        ));
    }
}

/// `pwrite` io operation.
pub struct PositionedWriteBytes<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    offset: usize,
    io_request_data: Option<IoRequestData>,
}

impl<'buf> PositionedWriteBytes<'buf> {
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

impl Future for PositionedWriteBytes<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().pwrite(
                this.fd,
                this.buf.as_ptr(),
                this.buf.len() as u32,
                this.offset,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}
/// `pwrite` io operation.
pub struct PositionedWriteFixed<'buf> {
    fd: RawFd,
    ptr: *const u8,
    len: u32,
    fixed_index: u16,
    offset: usize,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'buf Buffer>,
}

impl<'buf> PositionedWriteFixed<'buf> {
    /// Creates a new `pwrite` io operation.
    pub fn new(fd: RawFd, ptr: *const u8, len: u32, fixed_index: u16, offset: usize) -> Self {
        Self {
            fd,
            ptr,
            len,
            fixed_index,
            offset,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl Future for PositionedWriteFixed<'_> {
    type Output = Result<u32>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().pwrite_fixed(
                this.fd,
                this.ptr,
                this.len,
                this.fixed_index,
                this.offset,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret as u32
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
/// use orengine::io::{buffer, AsyncWrite};
///
/// # fn fill_buffer(buffer: &mut orengine::io::Buffer) {}
///
/// # async fn foo() -> std::io::Result<()> {
/// let options = OpenOptions::new().write(true);
/// let mut file = File::open("example.txt", &options).await?;
/// let mut buffer = buffer();
///
/// fill_buffer(&mut buffer);
///
/// // Asynchronously write to the file
/// file.write_all(&buffer).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncWrite: AsRawFd {
    /// Asynchronously writes data from the provided byte slice to the file descriptor.
    ///
    /// This method write some bytes from the byte slice to the file descriptor.
    /// It returns a future that resolves to the number of bytes written.
    ///
    /// # Difference between `write` and `write_bytes`
    ///
    /// Use [`write`](Self::write) if it is possible, because [`Buffer`] can be __fixed__.
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
    /// let bytes_written = file.write_bytes(b"Hello, world!").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn write_bytes(&mut self, buf: &[u8]) -> impl Future<Output = Result<usize>> {
        WriteBytes::new(self.as_raw_fd(), buf)
    }

    /// Asynchronously writes data from the provided [`Buffer`] to the file descriptor.
    ///
    /// This method write some bytes from the [`Buffer`] to the file descriptor.
    /// It returns a future that resolves to the number of bytes written.
    ///
    /// # Difference between `write` and `write_bytes`
    ///
    /// Use [`write`](Self::write) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::File;
    /// use orengine::fs::OpenOptions;
    /// use orengine::io::{buffer, AsyncWrite};
    ///
    /// # fn fill_buffer(buffer: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// let bytes_written = file.write(&buffer).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn write(&mut self, buf: &impl FixedBuffer) -> Result<u32> {
        if buf.is_fixed() {
            WriteFixed::new(
                self.as_raw_fd(),
                buf.as_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
            )
            .await
        } else {
            WriteBytes::new(self.as_raw_fd(), buf.as_bytes())
                .await
                .map(|r| r as u32)
        }
    }

    /// Asynchronously performs a positioned write, writing the provided byte slice to the file
    /// at the specified offset.
    ///
    /// This method does not modify the file's current position but instead writes to the specified
    /// `offset`. It returns a future that resolves to the number of bytes written.
    ///
    /// # Difference between `pwrite` and `pwrite_bytes`
    ///
    /// Use [`pwrite`](Self::pwrite) if it is possible, because [`Buffer`] can be __fixed__.
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
    /// let bytes_written = file.pwrite_bytes(b"Hello, world!", 1024).await?;  // Write at offset 1024
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn pwrite_bytes(&mut self, buf: &[u8], offset: usize) -> impl Future<Output = Result<usize>> {
        PositionedWriteBytes::new(self.as_raw_fd(), buf, offset)
    }

    /// Asynchronously performs a positioned write, writing the provided [`Buffer`]
    /// to the file at the specified offset.
    ///
    /// This method does not modify the file's current position but instead writes to the specified
    /// `offset`. It returns a future that resolves to the number of bytes written.
    ///
    /// # Difference between `pwrite` and `pwrite_bytes`
    ///
    /// Use [`pwrite`](Self::pwrite) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::File;
    /// use orengine::fs::OpenOptions;
    /// use orengine::io::{buffer, AsyncWrite};
    ///
    /// # fn fill_buffer(buffer: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// let bytes_written = file.pwrite(&buffer, 1024).await?;  // Write at offset 1024
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn pwrite(&mut self, buf: &impl FixedBuffer, offset: usize) -> Result<u32> {
        if buf.is_fixed() {
            PositionedWriteFixed::new(
                self.as_raw_fd(),
                buf.as_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
                offset,
            )
            .await
        } else {
            PositionedWriteBytes::new(self.as_raw_fd(), buf.as_bytes(), offset)
                .await
                .map(|r| r as u32)
        }
    }

    /// Asynchronously writes the entire provided byte slice to the file descriptor.
    ///
    /// This method continues writing until the entire byte slice is written to the file descriptor.
    /// It will return an error if the write operation fails or cannot complete.
    ///
    /// # Difference between `write_all` and `write_all_bytes`
    ///
    /// Use [`write_all`](Self::write_all) if it is possible, because [`Buffer`] can be __fixed__.
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
    /// file.write_all_bytes(b"Hello, world").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn write_all_bytes(&mut self, buf: &[u8]) -> Result<()> {
        let mut written = 0;

        while written < buf.len() {
            written += self.write_bytes(&buf[written..]).await?;
        }

        Ok(())
    }

    /// Asynchronously writes the entire provided [`Buffer`] to the file descriptor.
    ///
    /// This method continues writing until the entire [`Buffer`] is written to the file descriptor.
    /// It will return an error if the write operation fails or cannot complete.
    ///
    /// # Difference between `write_all` and `write_all_bytes`
    ///
    /// Use [`write_all`](Self::write_all) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::File;
    /// use orengine::fs::OpenOptions;
    /// use orengine::io::{buffer, AsyncWrite};
    ///
    /// # fn fill_buffer(buffer: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// file.write_all(&buffer.slice(..10)).await?; // Write exactly the first 10 bytes or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn write_all(&mut self, buf: &impl FixedBuffer) -> Result<()> {
        if buf.is_fixed() {
            let mut written = 0;

            while written < buf.len_u32() {
                written += WriteFixed::new(
                    self.as_raw_fd(),
                    unsafe { buf.as_ptr().offset(written as isize) },
                    buf.len_u32() - written,
                    buf.fixed_index(),
                )
                .await?;
            }
        } else {
            let mut written = 0;
            let slice = buf.as_bytes();

            while written < slice.len() {
                written += self.write_bytes(&slice[written..]).await?;
            }
        }

        Ok(())
    }

    /// Asynchronously performs a positioned write, writing the entire provided byte slice
    /// from the buffer starting at the specified offset.
    ///
    /// This method continues writing from the specified `offset` until the entire byte slice
    /// is written. If the write operation fails or cannot complete, it will return an error.
    ///
    /// # Difference between `pwrite_all` and `pwrite_all_bytes`
    ///
    /// Use [`pwrite_all`](Self::pwrite_all) if it is possible, because [`Buffer`] can be __fixed__.
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
    /// file.pwrite_all_bytes(b"Hello, world", 512).await?;  // Write all starting at offset 512
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn pwrite_all_bytes(&mut self, buf: &[u8], offset: usize) -> Result<()> {
        let mut written = 0;
        while written < buf.len() {
            written += self.pwrite_bytes(&buf[written..], offset + written).await?;
        }
        Ok(())
    }

    /// Asynchronously performs a positioned write, writing the entire provided [`Buffer`]
    /// from the buffer starting at the specified offset.
    ///
    /// This method continues writing from the specified `offset` until the entire [`Buffer`]
    /// is written. If the write operation fails or cannot complete, it will return an error.
    ///
    /// # Difference between `pwrite_all` and `pwrite_all_bytes`
    ///
    /// Use [`pwrite_all`](Self::pwrite_all) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::File;
    /// use orengine::fs::OpenOptions;
    /// use orengine::io::{buffer, AsyncWrite, Buffer};
    ///
    /// # fn fill_buffer(buffer: &mut Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().write(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// file.pwrite_all_bytes(&buffer.slice(..12), 512).await?;  // Write exactly the first 12 bytes
    /// // starting at offset 512 or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn pwrite_all(&mut self, buf: &impl FixedBuffer, offset: usize) -> Result<()> {
        if buf.is_fixed() {
            let mut written = 0;

            while written < buf.len_u32() {
                written += PositionedWriteFixed::new(
                    self.as_raw_fd(),
                    unsafe { buf.as_ptr().offset(written as isize) },
                    buf.len_u32() - written,
                    buf.fixed_index(),
                    offset + written as usize,
                )
                .await?;
            }
        } else {
            let mut written = 0;
            let slice = buf.as_bytes();

            while written < slice.len() {
                written += self
                    .pwrite_bytes(&slice[written..], offset + written)
                    .await?;
            }
        }

        Ok(())
    }
}

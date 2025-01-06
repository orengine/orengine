use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{AsRawFile, RawFile};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::{Buffer, FixedBufferMut};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future for the `read` operation.
pub struct ReadBytes<'buf> {
    raw_file: RawFile,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> ReadBytes<'buf> {
    /// Creates a new `read` io operation.
    pub fn new(raw_file: RawFile, buf: &'buf mut [u8]) -> Self {
        Self {
            raw_file,
            buf,
            io_request_data: None,
        }
    }
}

impl Future for ReadBytes<'_> {
    type Output = Result<usize>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never read more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().read(
                this.raw_file,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ret
        ));
    }
}

unsafe impl Send for ReadBytes<'_> {}

/// Future for the `read` operation with __fixed__ [`Buffer`].
pub struct ReadFixed<'buf> {
    raw_file: RawFile,
    ptr: *mut u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'buf Buffer>,
}

impl ReadFixed<'_> {
    /// Creates a new `read` io operation with __fixed__ [`Buffer`].
    pub fn new(raw_file: RawFile, ptr: *mut u8, len: u32, fixed_index: u16) -> Self {
        Self {
            raw_file,
            ptr,
            len,
            fixed_index,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl Future for ReadFixed<'_> {
    type Output = Result<u32>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never read more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().read_fixed(
                this.raw_file,
                this.ptr,
                this.len,
                this.fixed_index,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ret as u32
        ));
    }
}

unsafe impl Send for ReadFixed<'_> {}

/// Future for the `pread` operation.
///
/// # Positional read
///
/// This is a variation of `read` that allows
/// to specify the offset from which the data should be read.
pub struct PositionedReadBytes<'buf> {
    raw_file: RawFile,
    buf: &'buf mut [u8],
    offset: usize,
    io_request_data: Option<IoRequestData>,
}

impl<'buf> PositionedReadBytes<'buf> {
    /// Creates a new `pread` io operation.
    pub fn new(raw_file: RawFile, buf: &'buf mut [u8], offset: usize) -> Self {
        Self {
            raw_file,
            buf,
            offset,
            io_request_data: None,
        }
    }
}

impl Future for PositionedReadBytes<'_> {
    type Output = Result<usize>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never read more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().pread(
                this.raw_file,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                this.offset,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ret
        ));
    }
}

unsafe impl Send for PositionedReadBytes<'_> {}

/// Future for the `pread` operation with __fixed__ [`Buffer`].
///
/// # Positional read
///
/// This is a variation of `read` that allows
/// to specify the offset from which the data should be read.
pub struct PositionedReadFixed<'buf> {
    raw_file: RawFile,
    ptr: *mut u8,
    len: u32,
    fixed_index: u16,
    offset: usize,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'buf Buffer>,
}

impl PositionedReadFixed<'_> {
    /// Creates a new `pread` io operation with __fixed__ [`Buffer`].
    pub fn new(raw_file: RawFile, ptr: *mut u8, len: u32, fixed_index: u16, offset: usize) -> Self {
        Self {
            raw_file,
            ptr,
            len,
            fixed_index,
            offset,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl Future for PositionedReadFixed<'_> {
    type Output = Result<u32>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never read more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().pread_fixed(
                this.raw_file,
                this.ptr,
                this.len,
                this.fixed_index,
                this.offset,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ret as u32
        ));
    }
}

unsafe impl Send for PositionedReadFixed<'_> {}

/// The `AsyncRead` trait provides asynchronous methods for reading bytes from readers.
///
/// This trait is implemented for types that can be represented
/// as raw file descriptors (via [`AsRawFile`]).
///
/// It includes basic asynchronous read operations,
/// as well as methods for performing positioned reads.
///
/// # Example
///
/// ```rust
/// use std::ops::Deref;
/// use orengine::io::full_buffer;
/// use orengine::fs::{File, OpenOptions};
/// use orengine::io::{AsyncRead, AsyncWrite};
///
/// # async fn foo() -> std::io::Result<()> {
/// let options = OpenOptions::new()
///                 .read(true)
///                 .write(true)
///                 .create(true);
/// let mut file = File::open("example.txt", &options).await?;
/// let mut buffer = full_buffer();
/// buffer.append(b"Hello world!");
/// file.write_all(&buffer).await?;
///
/// // Asynchronously read into buffer
/// let bytes_read = file.read(&mut buffer).await?;
/// // Asynchronously read exactly 12 bytes
/// file.read_exact(&mut buffer.slice_mut(..12)).await?;
/// assert_eq!(&buffer[..12], b"Hello world!");
///
/// let bytes_read = file.pread(&mut buffer, 6).await?;
/// // or read exactly 6 bytes
/// file.pread_exact(&mut buffer, 6).await?;
/// assert_eq!(&buffer[..6], b"world!");
/// # Ok(())
/// # }
/// ```
pub trait AsyncRead: AsRawFile {
    /// Asynchronously reads data from the reader into the provided byte slice.
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
    fn read_bytes(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<usize>> {
        ReadBytes::new(self.as_raw_file(), buf)
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
    /// use orengine::io::{full_buffer, AsyncRead};
    /// use orengine::fs::{File, OpenOptions};
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
    async fn read(&mut self, buf: &mut impl FixedBufferMut) -> Result<u32> {
        if buf.is_fixed() {
            ReadFixed::new(
                self.as_raw_file(),
                buf.as_mut_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
            )
            .await
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "It never read more than u32::MAX bytes"
            )]
            ReadBytes::new(self.as_raw_file(), buf.as_bytes_mut())
                .await
                .map(|r| r as u32)
        }
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
    /// use orengine::io::AsyncRead;
    /// use orengine::fs::{File, OpenOptions};
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
    fn pread_bytes(
        &mut self,
        buf: &mut [u8],
        offset: usize,
    ) -> impl Future<Output = Result<usize>> {
        PositionedReadBytes::new(self.as_raw_file(), buf, offset)
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
    /// use orengine::io::{full_buffer, AsyncRead};
    /// use orengine::fs::{File, OpenOptions};
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
    async fn pread(&mut self, buf: &mut impl FixedBufferMut, offset: usize) -> Result<u32> {
        if buf.is_fixed() {
            PositionedReadFixed::new(
                self.as_raw_file(),
                buf.as_mut_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
                offset,
            )
            .await
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "It never read more than u32::MAX bytes"
            )]
            PositionedReadBytes::new(self.as_raw_file(), buf.as_bytes_mut(), offset)
                .await
                .map(|ret| ret as u32)
        }
    }

    /// Asynchronously reads the exact number of bytes required to fill the byte slice.
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

            // TODO: remove
            debug_assert!(read > 0);
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
    /// use orengine::io::{full_buffer, AsyncRead};
    /// use orengine::fs::{File, OpenOptions};
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
    async fn read_exact(&mut self, buf: &mut impl FixedBufferMut) -> Result<()> {
        if buf.is_fixed() {
            let mut read = 0;

            #[allow(
                clippy::cast_possible_wrap,
                reason = "We believe it never read u32::MAX bytes"
            )]
            while read < buf.len_u32() {
                read += ReadFixed::new(
                    self.as_raw_file(),
                    unsafe { buf.as_mut_ptr().offset(read as isize) },
                    buf.len_u32() - read,
                    buf.fixed_index(),
                )
                .await?;
            }
        } else {
            let mut read = 0;
            let slice = buf.as_bytes_mut();

            while read < slice.len() {
                read += self.read_bytes(&mut slice[read..]).await?;
            }
        }

        Ok(())
    }

    /// Asynchronously performs a positioned read, reading exactly the number of bytes needed to fill the byte slice.
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
    /// use orengine::io::{full_buffer, AsyncRead};
    /// use orengine::fs::{File, OpenOptions};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let options = OpenOptions::new().read(true);
    /// let mut file = File::open("example.txt", &options).await?;
    /// let mut buffer = full_buffer();
    /// file.pread_exact(&mut buffer.slice_mut(..13), 512).await?;  // Read exactly 13 bytes starting from offset 512
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn pread_exact(&mut self, buf: &mut impl FixedBufferMut, offset: usize) -> Result<()> {
        if buf.is_fixed() {
            let mut read = 0;

            #[allow(
                clippy::cast_possible_wrap,
                reason = "We believe it never read u32::MAX bytes"
            )]
            while read < buf.len_u32() {
                read += PositionedReadFixed::new(
                    self.as_raw_file(),
                    unsafe { buf.as_mut_ptr().offset(read as isize) },
                    buf.len_u32() - read,
                    buf.fixed_index(),
                    offset + read as usize,
                )
                .await?;
            }
        } else {
            let mut read = 0;
            let slice = buf.as_bytes_mut();

            while read < slice.len() {
                read += self.pread_bytes(&mut slice[read..], offset + read).await?;
            }
        }

        Ok(())
    }
}

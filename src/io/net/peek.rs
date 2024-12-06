use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::Buffer;

/// `peek` io operation.
pub struct PeekBytes<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> PeekBytes<'buf> {
    /// Creates a new `peek` io operation.
    pub fn new(fd: RawFd, buf: &'buf mut [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request_data: None,
        }
    }
}

impl Future for PeekBytes<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().peek(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

/// `peek` io operation with __fixed__ [`Buffer`].
pub struct PeekFixed<'buf> {
    fd: RawFd,
    ptr: *mut u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    phantom_data: std::marker::PhantomData<&'buf Buffer>,
}

impl<'buf> PeekFixed<'buf> {
    /// Creates a new `peek` io operation with __fixed__ [`Buffer`].
    pub fn new(fd: RawFd, ptr: *mut u8, len: u32, fixed_index: u16) -> Self {
        Self {
            fd,
            ptr,
            len,
            fixed_index,
            io_request_data: None,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl Future for PeekFixed<'_> {
    type Output = Result<u32>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().peek_fixed(this.fd, this.ptr, this.len, this.fixed_index, unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ret as u32
        ));
    }
}

/// `peek` io operation with deadline.
pub struct PeekBytesWithDeadline<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
}

impl<'buf> PeekBytesWithDeadline<'buf> {
    /// Creates a new `peek` io operation.
    pub fn new(fd: RawFd, buf: &'buf mut [u8], deadline: Instant) -> Self {
        Self {
            fd,
            buf,
            io_request_data: None,
            deadline,
        }
    }
}

impl Future for PeekBytesWithDeadline<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.peek_with_deadline(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() },
                &mut this.deadline
            ),
            ret
        ));
    }
}

/// `peek` io operation with __fixed__ [`Buffer`] with deadline.
pub struct PeekFixedWithDeadline<'buf> {
    fd: RawFd,
    ptr: *mut u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
    phantom_data: std::marker::PhantomData<&'buf Buffer>,
}

impl<'buf> PeekFixedWithDeadline<'buf> {
    /// Creates a new `peek` io operation with __fixed__ [`Buffer`].
    pub fn new(fd: RawFd, ptr: *mut u8, len: u32, fixed_index: u16, deadline: Instant) -> Self {
        Self {
            fd,
            ptr,
            len,
            fixed_index,
            io_request_data: None,
            deadline,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl Future for PeekFixedWithDeadline<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.peek_fixed_with_deadline(
                this.fd,
                this.ptr,
                this.len,
                this.fixed_index,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() },
                &mut this.deadline
            ),
            ret
        ));
    }
}

/// The `AsyncPeek` trait provides asynchronous methods for peeking at the incoming data
/// without consuming it.
///
/// It offers options to peek with deadlines, timeouts, and to ensure
/// reading an exact number of bytes.
///
/// This trait can be implemented for any socket that supports the `AsRawFd` and can be connected.
///
/// # Example
///
/// ```rust
/// use orengine::buf::full_buffer;
/// use orengine::net::TcpStream;
/// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
/// stream.poll_recv().await?;
/// let mut buf = full_buffer();
///
/// // Peek at the incoming data without consuming it
/// let bytes_peeked = stream.peek(&mut buf).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncPeek: AsRawFd {
    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// filling the buffer with available data. Returns the number of bytes peeked.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::buf::full_buffer;
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// stream.poll_recv().await?;
    /// let mut buf = full_buffer();
    /// let bytes_peeked = stream.peek(&mut buf).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek(&mut self, buf: &mut [u8]) -> Result<usize> {
        PeekBytes::new(self.as_raw_fd(), buf).await
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a specified deadline. Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::buf::full_buffer;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::{Duration, Instant};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// stream.poll_recv_with_deadline(deadline).await?;
    /// let mut buf = full_buffer();
    ///
    /// let bytes_peeked = stream.peek_with_deadline(&mut buf, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_with_deadline(&mut self, buf: &mut [u8], deadline: Instant) -> Result<usize> {
        PeekBytesWithDeadline::new(self.as_raw_fd(), buf, deadline).await
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a specified timeout. Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::buf::full_buffer;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// stream.poll_recv_with_timeout(timeout).await?;
    /// let mut buf = full_buffer();
    ///
    /// let bytes_peeked = stream.peek_with_timeout(&mut buf, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_with_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        self.peek_with_deadline(buf, Instant::now() + timeout).await
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// until the buffer is completely filled with exactly the requested number of bytes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::buf::full_buffer;
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// stream.poll_recv().await?;
    /// let mut buf = full_buffer();
    ///
    /// stream.peek_exact(&mut buf[..100]).await?; // Peek 100 bytes
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut peeked = 0;

        while peeked < buf.len() {
            peeked += self.peek(&mut buf[peeked..]).await?;
        }
        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a deadline until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use orengine::buf::full_buffer;
    /// use std::time::{Instant, Duration};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// stream.poll_recv_with_deadline(deadline).await?;
    /// let mut buf = full_buffer();
    ///
    /// stream.peek_exact_with_deadline(&mut buf[..100], deadline).await?; // Peek 100 bytes
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_exact_with_deadline(&mut self, buf: &mut [u8], deadline: Instant) -> Result<()> {
        let mut peeked = 0;

        while peeked < buf.len() {
            peeked += self
                .peek_with_deadline(&mut buf[peeked..], deadline)
                .await?;
        }
        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a timeout until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use orengine::buf::full_buffer;
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// stream.poll_recv_with_timeout(timeout).await?;
    /// let mut buf = full_buffer();
    ///
    /// stream.peek_exact_with_timeout(&mut buf[..100], timeout).await?; // Peek 100 bytes
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_exact_with_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()> {
        self.peek_exact_with_deadline(buf, Instant::now() + timeout)
            .await
    }
}

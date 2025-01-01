use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawSocket, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::{Buffer, FixedBufferMut};

/// `peek` io operation.
pub struct PeekBytes<'buf> {
    raw_socket: RawSocket,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> PeekBytes<'buf> {
    /// Creates a new `peek` io operation.
    pub fn new(raw_socket: RawSocket, buf: &'buf mut [u8]) -> Self {
        Self {
            raw_socket,
            buf,
            io_request_data: None,
        }
    }
}

impl Future for PeekBytes<'_> {
    type Output = Result<usize>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never peek more than u32::MAX"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().peek(
                this.raw_socket,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

unsafe impl Send for PeekBytes<'_> {}

/// `peek` io operation with __fixed__ [`Buffer`].
pub struct PeekFixed<'buf> {
    raw_socket: RawSocket,
    ptr: *mut u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    phantom_data: std::marker::PhantomData<&'buf Buffer>,
}

impl PeekFixed<'_> {
    /// Creates a new `peek` io operation with __fixed__ [`Buffer`].
    pub fn new(raw_socket: RawSocket, ptr: *mut u8, len: u32, fixed_index: u16) -> Self {
        Self {
            raw_socket,
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

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never peek more than u32::MAX"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().peek_fixed(
                this.raw_socket,
                this.ptr,
                this.len,
                this.fixed_index,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret as u32
        ));
    }
}

unsafe impl Send for PeekFixed<'_> {}

/// `peek` io operation with deadline.
pub struct PeekBytesWithDeadline<'buf> {
    raw_socket: RawSocket,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
}

impl<'buf> PeekBytesWithDeadline<'buf> {
    /// Creates a new `peek` io operation.
    pub fn new(raw_socket: RawSocket, buf: &'buf mut [u8], deadline: Instant) -> Self {
        Self {
            raw_socket,
            buf,
            io_request_data: None,
            deadline,
        }
    }
}

impl Future for PeekBytesWithDeadline<'_> {
    type Output = Result<usize>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never peek more than u32::MAX"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.peek_with_deadline(
                this.raw_socket,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() },
                &mut this.deadline
            ),
            ret
        ));
    }
}

unsafe impl Send for PeekBytesWithDeadline<'_> {}

/// `peek` io operation with __fixed__ [`Buffer`] with deadline.
pub struct PeekFixedWithDeadline<'buf> {
    raw_socket: RawSocket,
    ptr: *mut u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
    phantom_data: std::marker::PhantomData<&'buf Buffer>,
}

impl PeekFixedWithDeadline<'_> {
    /// Creates a new `peek` io operation with __fixed__ [`Buffer`].
    pub fn new(
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        fixed_index: u16,
        deadline: Instant,
    ) -> Self {
        Self {
            raw_socket,
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
    type Output = Result<u32>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never peek more than u32::MAX"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.peek_fixed_with_deadline(
                this.raw_socket,
                this.ptr,
                this.len,
                this.fixed_index,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() },
                &mut this.deadline
            ),
            ret as u32
        ));
    }
}

unsafe impl Send for PeekFixedWithDeadline<'_> {}

/// The `AsyncPeek` trait provides asynchronous methods for peeking at the incoming data
/// without consuming it.
///
/// It offers options to peek with deadlines, timeouts, and to ensure
/// reading an exact number of bytes.
///
/// This trait can be implemented for any socket that supports the `AsRawSocket` and can be connected.
///
/// # Example
///
/// ```rust
/// use orengine::net::TcpStream;
/// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPeek, AsyncPollFd};
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
/// stream.poll_recv().await?;
///
/// let mut buf = full_buffer();
/// let bytes_peeked = stream.peek(&mut buf).await?; // Peek at the incoming data without consuming it
/// # Ok(())
/// # }
/// ```
pub trait AsyncPeek: AsRawSocket {
    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// filling the buffer with available data. Returns the number of bytes peeked.
    ///
    /// # Difference between `peek` and `peek_bytes`
    ///
    /// Use [`peek`](Self::peek) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.poll_recv().await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// let bytes_peeked = stream.peek_bytes(&mut vec).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peek_bytes(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<usize>> {
        PeekBytes::new(self.as_raw_socket(), buf)
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// filling the buffer with available data. Returns the number of bytes peeked.
    ///
    /// # Difference between `peek` and `peek_bytes`
    ///
    /// Use [`peek`](Self::peek) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPeek, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.poll_recv().await?;
    ///
    /// let mut buffer = full_buffer();
    /// let bytes_peeked = stream.peek(&mut buffer).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek(&mut self, buf: &mut impl FixedBufferMut) -> Result<u32> {
        if buf.is_fixed() {
            PeekFixed::new(
                self.as_raw_socket(),
                buf.as_mut_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
            )
            .await
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "It never peek more than u32::MAX"
            )]
            PeekBytes::new(self.as_raw_socket(), buf.as_bytes_mut())
                .await
                .map(|r| r as u32)
        }
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a specified deadline. Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `peek_with_deadline` and `peek_bytes_with_deadline`
    ///
    /// Use [`peek_with_deadline`](Self::peek_with_deadline) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::{Duration, Instant};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_deadline(deadline).await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// let bytes_peeked = stream.peek_bytes_with_deadline(&mut vec, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peek_bytes_with_deadline(
        &mut self,
        buf: &mut [u8],
        deadline: Instant,
    ) -> impl Future<Output = Result<usize>> {
        PeekBytesWithDeadline::new(self.as_raw_socket(), buf, deadline)
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a specified deadline. Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `peek_with_deadline` and `peek_bytes_with_deadline`
    ///
    /// Use [`peek_with_deadline`](Self::peek_with_deadline) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::{Duration, Instant};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_deadline(deadline).await?;
    ///
    /// let mut buffer = full_buffer();
    /// let bytes_peeked = stream.peek_with_deadline(&mut buffer, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_with_deadline(
        &mut self,
        buf: &mut impl FixedBufferMut,
        deadline: Instant,
    ) -> Result<u32> {
        if buf.is_fixed() {
            PeekFixedWithDeadline::new(
                self.as_raw_socket(),
                buf.as_mut_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
                deadline,
            )
            .await
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "It never peek more than u32::MAX"
            )]
            PeekBytesWithDeadline::new(self.as_raw_socket(), buf.as_bytes_mut(), deadline)
                .await
                .map(|r| r as u32)
        }
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a specified timeout. Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `peek_with_timeout` and `peek_bytes_with_timeout`
    ///
    /// Use [`peek_with_timeout`](Self::peek_with_timeout) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_timeout(timeout).await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// let bytes_peeked = stream.peek_bytes_with_timeout(&mut vec, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peek_bytes_with_timeout(
        &mut self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> impl Future<Output = Result<usize>> {
        self.peek_bytes_with_deadline(buf, Instant::now() + timeout)
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a specified timeout. Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `peek_with_timeout` and `peek_bytes_with_timeout`
    ///
    /// Use [`peek_with_timeout`](Self::peek_with_timeout) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_timeout(timeout).await?;
    ///
    /// let mut buffer = full_buffer();
    /// let bytes_peeked = stream.peek_with_timeout(&mut buffer, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peek_with_timeout(
        &mut self,
        buf: &mut impl FixedBufferMut,
        timeout: Duration,
    ) -> impl Future<Output = Result<u32>> {
        self.peek_with_deadline(buf, Instant::now() + timeout)
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// until the buffer is completely filled with exactly the requested number of bytes.
    ///
    /// # Difference between `peek_exact` and `peek_bytes_exact`
    ///
    /// Use [`peek_exact`](Self::peek_exact) if it is possible,
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.poll_recv().await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// stream.peek_bytes_exact(&mut vec[..100]).await?; // Peek exactly 100 bytes or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_bytes_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut peeked = 0;

        while peeked < buf.len() {
            peeked += self.peek_bytes(&mut buf[peeked..]).await?;
        }
        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// until the buffer is completely filled with exactly the requested number of bytes.
    ///
    /// # Difference between `peek_exact` and `peek_bytes_exact`
    ///
    /// Use [`peek_exact`](Self::peek_exact) if it is possible,
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPeek, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.poll_recv().await?;
    ///
    /// let mut buffer = full_buffer();
    /// stream.peek_exact(&mut buffer.slice_mut(..100)).await?; // Peek exactly 100 bytes or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_exact(&mut self, buf: &mut impl FixedBufferMut) -> Result<()> {
        if buf.is_fixed() {
            let mut peeked = 0;

            #[allow(
                clippy::cast_possible_wrap,
                reason = "We believe it never peek u32::MAX bytes"
            )]
            while peeked < buf.len_u32() {
                peeked += PeekFixed::new(
                    self.as_raw_socket(),
                    unsafe { buf.as_mut_ptr().offset(peeked as isize) },
                    buf.len_u32() - peeked,
                    buf.fixed_index(),
                )
                .await?;
            }
        } else {
            let mut peeked = 0;
            let slice = buf.as_bytes_mut();

            while peeked < slice.len() {
                peeked += self.peek_bytes(&mut slice[peeked..]).await?;
            }
        }

        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a deadline until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `peek_exact_with_deadline` and `peek_bytes_exact_with_deadline`
    ///
    /// Use [`peek_exact_with_deadline`](Self::peek_exact_with_deadline) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::{Instant, Duration};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_deadline(deadline).await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// stream.peek_bytes_exact_with_deadline(&mut vec[..100], deadline).await?; // Peek exactly 100 bytes
    /// // or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_bytes_exact_with_deadline(
        &mut self,
        buf: &mut [u8],
        deadline: Instant,
    ) -> Result<()> {
        let mut peeked = 0;

        while peeked < buf.len() {
            peeked += self
                .peek_bytes_with_deadline(&mut buf[peeked..], deadline)
                .await?;
        }
        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a deadline until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `peek_exact_with_deadline` and `peek_bytes_exact_with_deadline`
    ///
    /// Use [`peek_exact_with_deadline`](Self::peek_exact_with_deadline) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::{Instant, Duration};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_deadline(deadline).await?;
    ///
    /// let mut vec = full_buffer();
    /// stream.peek_exact_with_deadline(&mut vec.slice_mut(..100), deadline).await?; // Peek exactly 100 bytes
    /// // or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn peek_exact_with_deadline(
        &mut self,
        buf: &mut impl FixedBufferMut,
        deadline: Instant,
    ) -> Result<()> {
        if buf.is_fixed() {
            let mut peeked = 0;

            #[allow(
                clippy::cast_possible_wrap,
                reason = "We believe it never peek u32::MAX bytes"
            )]
            while peeked < buf.len_u32() {
                peeked += PeekFixedWithDeadline::new(
                    self.as_raw_socket(),
                    unsafe { buf.as_mut_ptr().offset(peeked as isize) },
                    buf.len_u32() - peeked,
                    buf.fixed_index(),
                    deadline,
                )
                .await?;
            }
        } else {
            let mut peeked = 0;
            let slice = buf.as_bytes_mut();

            while peeked < slice.len() {
                peeked += self
                    .peek_bytes_with_deadline(&mut slice[peeked..], deadline)
                    .await?;
            }
        }

        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a timeout until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `peek_exact_with_timeout` and `peek_bytes_exact_with_timeout`
    ///
    /// Use [`peek_exact_with_timeout`](Self::peek_exact_with_timeout) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_timeout(timeout).await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// stream.peek_bytes_exact_with_timeout(&mut vec[..100], timeout).await?; // Peek exactly 100 bytes
    /// // or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peek_bytes_exact_with_timeout(
        &mut self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> impl Future<Output = Result<()>> {
        self.peek_bytes_exact_with_deadline(buf, Instant::now() + timeout)
    }

    /// Asynchronously receives into the provided byte slice the incoming data without consuming it,
    /// with a timeout until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `peek_exact_with_timeout` and `peek_bytes_exact_with_timeout`
    ///
    /// Use [`peek_exact_with_timeout`](Self::peek_exact_with_timeout) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPeek, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_timeout(timeout).await?;
    ///
    /// let mut buffer = full_buffer();
    /// stream.peek_exact_with_timeout(&mut buffer.slice_mut(..100), timeout).await?; // Peek exactly 100 bytes
    /// // or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peek_exact_with_timeout(
        &mut self,
        buf: &mut impl FixedBufferMut,
        timeout: Duration,
    ) -> impl Future<Output = Result<()>> {
        self.peek_exact_with_deadline(buf, Instant::now() + timeout)
    }
}

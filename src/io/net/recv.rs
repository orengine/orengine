use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawSocket, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::FixedBufferMut;

/// `recv` io operation.
pub struct RecvBytes<'buf> {
    raw_socket: RawSocket,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> RecvBytes<'buf> {
    /// Creates a new `recv` io operation.
    pub fn new(raw_socket: RawSocket, buf: &'buf mut [u8]) -> Self {
        Self {
            raw_socket,
            buf,
            io_request_data: None,
        }
    }
}

impl Future for RecvBytes<'_> {
    type Output = Result<usize>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never receive more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().recv(
                this.raw_socket,
                this.buf.as_mut_ptr(),
                this.buf.len() as u32,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

unsafe impl Send for RecvBytes<'_> {}

/// `recv` io operation with __fixed__ [`Buffer`](crate::io::Buffer).
pub struct RecvFixed<'buf> {
    raw_socket: RawSocket,
    ptr: *mut u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'buf [u8]>,
}

impl RecvFixed<'_> {
    /// Creates a new `recv` io operation with __fixed__ [`Buffer`](crate::io::Buffer).
    pub fn new(raw_socket: RawSocket, ptr: *mut u8, len: u32, fixed_index: u16) -> Self {
        Self {
            raw_socket,
            ptr,
            len,
            fixed_index,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl Future for RecvFixed<'_> {
    type Output = Result<u32>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never receive more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().recv_fixed(
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

unsafe impl Send for RecvFixed<'_> {}

/// `recv` io operation with deadline.
pub struct RecvBytesWithDeadline<'buf> {
    raw_socket: RawSocket,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
}

impl<'buf> RecvBytesWithDeadline<'buf> {
    /// Creates a new `recv` io operation with deadline.
    pub fn new(raw_socket: RawSocket, buf: &'buf mut [u8], deadline: Instant) -> Self {
        Self {
            raw_socket,
            buf,
            io_request_data: None,
            deadline,
        }
    }
}

impl Future for RecvBytesWithDeadline<'_> {
    type Output = Result<usize>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never receive more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.recv_with_deadline(
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

unsafe impl Send for RecvBytesWithDeadline<'_> {}

/// `recv` io operation with deadline and __fixed__ [`Buffer`](crate::io::Buffer).
pub struct RecvFixedWithDeadline<'buf> {
    raw_socket: RawSocket,
    ptr: *mut u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
    phantom_data: PhantomData<&'buf [u8]>,
}

impl RecvFixedWithDeadline<'_> {
    /// Creates a new `recv` io operation with deadline and __fixed__ [`Buffer`](crate::io::Buffer).
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
            phantom_data: PhantomData,
        }
    }
}

impl Future for RecvFixedWithDeadline<'_> {
    type Output = Result<u32>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never receive more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.recv_fixed_with_deadline(
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

unsafe impl Send for RecvFixedWithDeadline<'_> {}

/// The `AsyncRecv` trait provides asynchronous methods for receiving at the incoming data
/// with consuming it.
///
/// It offers options to recv with deadlines, timeouts, and to ensure
/// reading an exact number of bytes.
///
/// This trait can be implemented for any socket that supports the `AsRawSocket` and can be connected.
///
/// # Example
///
/// ```rust
/// use orengine::net::TcpStream;
/// use orengine::io::{full_buffer, AsyncConnectStream, AsyncRecv, AsyncPollFd};
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
/// stream.poll_recv().await?;
///
/// let mut buf = full_buffer();
/// let bytes_received = stream.recv(&mut buf).await?; // Recv at the incoming data with consuming it
/// # Ok(())
/// # }
/// ```
pub trait AsyncRecv: AsRawSocket {
    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// filling the buffer with available data. Returns the number of bytes received.
    ///
    /// # Difference between `recv` and `recv_bytes`
    ///
    /// Use [`recv`](Self::recv) if it is possible, because [`Buffer`](crate::io::Buffer)
    /// can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncRecv, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.poll_recv().await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// let bytes_received = stream.recv_bytes(&mut vec).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn recv_bytes(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<usize>> {
        RecvBytes::new(self.as_raw_socket(), buf)
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// filling the buffer with available data. Returns the number of bytes received.
    ///
    /// # Difference between `recv` and `recv_bytes`
    ///
    /// Use [`recv`](Self::recv) if it is possible, because [`Buffer`](crate::io::Buffer)
    /// can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncRecv, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.poll_recv().await?;
    ///
    /// let mut buffer = full_buffer();
    /// let bytes_received = stream.recv(&mut buffer).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv(&mut self, buf: &mut impl FixedBufferMut) -> Result<u32> {
        if buf.is_fixed() {
            RecvFixed::new(
                self.as_raw_socket(),
                buf.as_mut_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
            )
            .await
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "It never receive more than u32::MAX bytes"
            )]
            RecvBytes::new(self.as_raw_socket(), buf.as_bytes_mut())
                .await
                .map(|r| r as u32)
        }
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// with a specified deadline. Returns the number of bytes received.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_with_deadline` and `recv_bytes_with_deadline`
    ///
    /// Use [`recv_with_deadline`](Self::recv_with_deadline) if it is possible,
    /// because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncRecv, AsyncPollFd};
    /// use std::time::{Duration, Instant};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_deadline(deadline).await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// let bytes_received = stream.recv_bytes_with_deadline(&mut vec, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn recv_bytes_with_deadline(
        &mut self,
        buf: &mut [u8],
        deadline: Instant,
    ) -> impl Future<Output = Result<usize>> {
        RecvBytesWithDeadline::new(self.as_raw_socket(), buf, deadline)
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// with a specified deadline. Returns the number of bytes received.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_with_deadline` and `recv_bytes_with_deadline`
    ///
    /// Use [`recv_with_deadline`](Self::recv_with_deadline) if it is possible,
    /// because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncRecv, AsyncPollFd};
    /// use std::time::{Duration, Instant};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_deadline(deadline).await?;
    ///
    /// let mut buffer = full_buffer();
    /// let bytes_received = stream.recv_with_deadline(&mut buffer, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_with_deadline(
        &mut self,
        buf: &mut impl FixedBufferMut,
        deadline: Instant,
    ) -> Result<u32> {
        if buf.is_fixed() {
            RecvFixedWithDeadline::new(
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
                reason = "It never receive more than u32::MAX bytes"
            )]
            RecvBytesWithDeadline::new(self.as_raw_socket(), buf.as_bytes_mut(), deadline)
                .await
                .map(|r| r as u32)
        }
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// with a specified timeout. Returns the number of bytes received.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_with_timeout` and `recv_bytes_with_timeout`
    ///
    /// Use [`recv_with_timeout`](Self::recv_with_timeout) if it is possible,
    /// because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncRecv, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_timeout(timeout).await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// let bytes_received = stream.recv_bytes_with_timeout(&mut vec, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn recv_bytes_with_timeout(
        &mut self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> impl Future<Output = Result<usize>> {
        self.recv_bytes_with_deadline(buf, Instant::now() + timeout)
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// with a specified timeout. Returns the number of bytes received.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_with_timeout` and `recv_bytes_with_timeout`
    ///
    /// Use [`recv_with_timeout`](Self::recv_with_timeout) if it is possible,
    /// because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncRecv, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_timeout(timeout).await?;
    ///
    /// let mut buffer = full_buffer();
    /// let bytes_received = stream.recv_with_timeout(&mut buffer, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn recv_with_timeout(
        &mut self,
        buf: &mut impl FixedBufferMut,
        timeout: Duration,
    ) -> impl Future<Output = Result<u32>> {
        self.recv_with_deadline(buf, Instant::now() + timeout)
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// until the buffer is completely filled with exactly the requested number of bytes.
    ///
    /// # Difference between `recv_exact` and `recv_bytes_exact`
    ///
    /// Use [`recv_exact`](Self::recv_exact) if it is possible,
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncRecv, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.poll_recv().await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// stream.recv_bytes_exact(&mut vec[..100]).await?; // Recv exactly 100 bytes or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_bytes_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut received = 0;

        while received < buf.len() {
            received += self.recv_bytes(&mut buf[received..]).await?;
        }
        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// until the buffer is completely filled with exactly the requested number of bytes.
    ///
    /// # Difference between `recv_exact` and `recv_bytes_exact`
    ///
    /// Use [`recv_exact`](Self::recv_exact) if it is possible,
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncRecv, AsyncPollFd};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.poll_recv().await?;
    ///
    /// let mut buffer = full_buffer();
    /// stream.recv_exact(&mut buffer.slice_mut(..100)).await?; // Recv exactly 100 bytes or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_exact(&mut self, buf: &mut impl FixedBufferMut) -> Result<()> {
        if buf.is_fixed() {
            let mut received = 0;

            #[allow(
                clippy::cast_possible_wrap,
                reason = "We believe it never receive u32::MAX bytes"
            )]
            while received < buf.len_u32() {
                received += RecvFixed::new(
                    self.as_raw_socket(),
                    unsafe { buf.as_mut_ptr().offset(received as isize) },
                    buf.len_u32() - received,
                    buf.fixed_index(),
                )
                .await?;
            }
        } else {
            let mut received = 0;
            let slice = buf.as_bytes_mut();

            while received < slice.len() {
                received += self.recv_bytes(&mut slice[received..]).await?;
            }
        }

        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// with a deadline until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_exact_with_deadline` and `recv_bytes_exact_with_deadline`
    ///
    /// Use [`recv_exact_with_deadline`](Self::recv_exact_with_deadline) if it is possible,
    /// because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncRecv, AsyncPollFd};
    /// use std::time::{Instant, Duration};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_deadline(deadline).await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// stream.recv_bytes_exact_with_deadline(&mut vec[..100], deadline).await?; // Recv exactly 100 bytes
    /// // or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_bytes_exact_with_deadline(
        &mut self,
        buf: &mut [u8],
        deadline: Instant,
    ) -> Result<()> {
        let mut received = 0;

        while received < buf.len() {
            received += self
                .recv_bytes_with_deadline(&mut buf[received..], deadline)
                .await?;
        }
        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// with a deadline until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_exact_with_deadline` and `recv_bytes_exact_with_deadline`
    ///
    /// Use [`recv_exact_with_deadline`](Self::recv_exact_with_deadline) if it is possible,
    /// because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncRecv, AsyncPollFd};
    /// use std::time::{Instant, Duration};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_deadline(deadline).await?;
    ///
    /// let mut vec = full_buffer();
    /// stream.recv_exact_with_deadline(&mut vec.slice_mut(..100), deadline).await?; // Recv exactly 100 bytes
    /// // or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_exact_with_deadline(
        &mut self,
        buf: &mut impl FixedBufferMut,
        deadline: Instant,
    ) -> Result<()> {
        if buf.is_fixed() {
            let mut received = 0;

            #[allow(
                clippy::cast_possible_wrap,
                reason = "We believe it never receive u32::MAX bytes"
            )]
            while received < buf.len_u32() {
                received += RecvFixedWithDeadline::new(
                    self.as_raw_socket(),
                    unsafe { buf.as_mut_ptr().offset(received as isize) },
                    buf.len_u32() - received,
                    buf.fixed_index(),
                    deadline,
                )
                .await?;
            }
        } else {
            let mut received = 0;
            let slice = buf.as_bytes_mut();

            while received < slice.len() {
                received += self
                    .recv_bytes_with_deadline(&mut slice[received..], deadline)
                    .await?;
            }
        }

        Ok(())
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// with a timeout until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_exact_with_timeout` and `recv_bytes_exact_with_timeout`
    ///
    /// Use [`recv_exact_with_timeout`](Self::recv_exact_with_timeout) if it is possible,
    /// because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncRecv, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_timeout(timeout).await?;
    ///
    /// let mut vec = vec![0u8; 1024];
    /// stream.recv_bytes_exact_with_timeout(&mut vec[..100], timeout).await?; // Recv exactly 100 bytes
    /// // or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn recv_bytes_exact_with_timeout(
        &mut self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> impl Future<Output = Result<()>> {
        self.recv_bytes_exact_with_deadline(buf, Instant::now() + timeout)
    }

    /// Asynchronously receives into the provided byte slice the incoming data with consuming it,
    /// with a timeout until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_exact_with_timeout` and `recv_bytes_exact_with_timeout`
    ///
    /// Use [`recv_exact_with_timeout`](Self::recv_exact_with_timeout) if it is possible,
    /// because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncRecv, AsyncPollFd};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    ///
    /// stream.poll_recv_with_timeout(timeout).await?;
    ///
    /// let mut buffer = full_buffer();
    /// stream.recv_exact_with_timeout(&mut buffer.slice_mut(..100), timeout).await?; // Recv exactly 100 bytes
    /// // or return an error
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn recv_exact_with_timeout(
        &mut self,
        buf: &mut impl FixedBufferMut,
        timeout: Duration,
    ) -> impl Future<Output = Result<()>> {
        self.recv_exact_with_deadline(buf, Instant::now() + timeout)
    }
}

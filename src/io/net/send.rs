use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{AsRawSocket, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::{Buffer, FixedBuffer};
use crate::local_executor;
use crate::net::Socket;

/// `send` io operation.
pub struct SendBytes<'buf> {
    raw_socket: RawSocket,
    buf: &'buf [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> SendBytes<'buf> {
    /// Creates new `send` io operation.
    pub fn new(raw_socket: RawSocket, buf: &'buf [u8]) -> Self {
        Self {
            raw_socket,
            buf,
            io_request_data: None,
        }
    }
}

impl Future for SendBytes<'_> {
    type Output = Result<usize>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never send more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().send(
                this.raw_socket,
                this.buf.as_ptr(),
                this.buf.len() as u32,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ret
        ));
    }
}

unsafe impl Send for SendBytes<'_> {}

/// `send` io operation.
pub struct SendFixed<'buf> {
    raw_socket: RawSocket,
    ptr: *const u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'buf Buffer>,
}

impl SendFixed<'_> {
    /// Creates new `send` io operation.
    pub fn new(raw_socket: RawSocket, ptr: *const u8, len: u32, fixed_index: u16) -> Self {
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

impl Future for SendFixed<'_> {
    type Output = Result<u32>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never send more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().send_fixed(
                this.raw_socket,
                this.ptr,
                this.len,
                this.fixed_index,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ret as u32
        ));
    }
}

unsafe impl Send for SendFixed<'_> {}

/// `send` io operation with deadline.
pub struct SendBytesWithDeadline<'buf> {
    raw_socket: RawSocket,
    buf: &'buf [u8],
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
}

impl<'buf> SendBytesWithDeadline<'buf> {
    /// Creates new `send` io operation with deadline.
    pub fn new(raw_socket: RawSocket, buf: &'buf [u8], deadline: Instant) -> Self {
        Self {
            raw_socket,
            buf,
            io_request_data: None,
            deadline,
        }
    }
}

impl Future for SendBytesWithDeadline<'_> {
    type Output = Result<usize>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never send more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.send_with_deadline(
                this.raw_socket,
                this.buf.as_ptr(),
                this.buf.len() as u32,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) },
                &mut this.deadline
            ),
            ret
        ));
    }
}

unsafe impl Send for SendBytesWithDeadline<'_> {}

/// `send` io operation with deadline.
pub struct SendFixedWithDeadline<'buf> {
    raw_socket: RawSocket,
    ptr: *const u8,
    len: u32,
    fixed_index: u16,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
    phantom_data: PhantomData<&'buf Buffer>,
}

impl SendFixedWithDeadline<'_> {
    /// Creates new `send` io operation with deadline.
    pub fn new(
        raw_socket: RawSocket,
        ptr: *const u8,
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

impl Future for SendFixedWithDeadline<'_> {
    type Output = Result<u32>;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "It never send more than u32::MAX bytes"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.send_fixed_with_deadline(
                this.raw_socket,
                this.ptr,
                this.len,
                this.fixed_index,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) },
                &mut this.deadline
            ),
            ret as u32
        ));
    }
}

unsafe impl Send for SendFixedWithDeadline<'_> {}

/// The `AsyncSend` trait provides asynchronous methods for sending data over a stream or socket.
///
/// It allows for sending data with or without deadlines, and ensures the complete transmission
/// of data when required.
///
/// This trait can be implemented for any sockets that supports the [`Socket`] trait
/// and can be connected.
///
/// # Example
///
/// ```rust
/// use orengine::net::TcpStream;
/// use orengine::io::{buffer, AsyncConnectStream, AsyncSend};
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
/// let mut data = buffer();
///
/// data.append(b"Hello, World!");
///
/// // Send data over the stream
/// let bytes_sent = stream.send(&data).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncSend: Socket {
    /// Asynchronously sends the provided byte slice. Returns the number of bytes sent.
    ///
    /// # Difference between `send` and `send_bytes`
    ///
    /// Use [`send`](Self::send) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let bytes_sent = stream.send_bytes(data).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn send_bytes(&mut self, buf: &[u8]) -> impl Future<Output = Result<usize>> {
        SendBytes::new(AsRawSocket::as_raw_socket(self), buf)
    }

    /// Asynchronously sends the provided [`Buffer`]. Returns the number of bytes sent.
    ///
    /// # Difference between `send` and `send_bytes`
    ///
    /// Use [`send`](Self::send) if it is possible, because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{buffer, AsyncConnectStream, AsyncSend};
    ///
    /// # fn fill_buffer(buf: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// let bytes_sent = stream.send(&buffer).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send(&mut self, buf: &impl FixedBuffer) -> Result<u32> {
        if buf.is_fixed() {
            SendFixed::new(
                AsRawSocket::as_raw_socket(self),
                buf.as_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
            )
            .await
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "It never send more than u32::MAX bytes"
            )]
            SendBytes::new(AsRawSocket::as_raw_socket(self), buf.as_bytes())
                .await
                .map(|r| r as u32)
        }
    }

    /// Asynchronously sends the provided byte slice with a specified deadline.
    /// Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `send_with_deadline` and `send_bytes_with_deadline`
    ///
    /// Use [`send_with_deadline`](Self::send_with_deadline) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    /// use std::time::{Duration, Instant};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// let bytes_sent = stream.send_bytes_with_deadline(data, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn send_bytes_with_deadline(
        &mut self,
        buf: &[u8],
        deadline: Instant,
    ) -> impl Future<Output = Result<usize>> {
        SendBytesWithDeadline::new(AsRawSocket::as_raw_socket(self), buf, deadline)
    }

    /// Asynchronously sends the provided [`Buffer`] with a specified deadline.
    /// Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `send_with_deadline` and `send_bytes_with_deadline`
    ///
    /// Use [`send_with_deadline`](Self::send_with_deadline) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{buffer, AsyncConnectStream, AsyncSend};
    /// use std::time::{Duration, Instant};
    ///
    /// # fn fill_buffer(buf: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// let bytes_sent = stream.send_with_deadline(&buffer, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_with_deadline(
        &mut self,
        buf: &impl FixedBuffer,
        deadline: Instant,
    ) -> Result<u32> {
        if buf.is_fixed() {
            SendFixedWithDeadline::new(
                AsRawSocket::as_raw_socket(self),
                buf.as_ptr(),
                buf.len_u32(),
                buf.fixed_index(),
                deadline,
            )
            .await
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "It never send more than u32::MAX bytes"
            )]
            SendBytesWithDeadline::new(AsRawSocket::as_raw_socket(self), buf.as_bytes(), deadline)
                .await
                .map(|r| r as u32)
        }
    }

    /// Asynchronously sends the provided byte slice with a specified timeout.
    /// Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `send_with_timeout` and `send_bytes_with_timeout`
    ///
    /// Use [`send_with_timeout`](Self::send_with_timeout) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let timeout = Duration::from_secs(5);
    /// let bytes_sent = stream.send_bytes_with_timeout(data, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn send_bytes_with_timeout(
        &mut self,
        buf: &[u8],
        timeout: Duration,
    ) -> impl Future<Output = Result<usize>> {
        SendBytesWithDeadline::new(
            AsRawSocket::as_raw_socket(self),
            buf,
            local_executor().start_round_time_for_deadlines() + timeout,
        )
    }

    /// Asynchronously sends the provided [`Buffer`] with a specified timeout.
    /// Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `send_with_timeout` and `send_bytes_with_timeout`
    ///
    /// Use [`send_with_timeout`](Self::send_with_timeout) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{buffer, AsyncConnectStream, AsyncSend};
    /// use std::time::Duration;
    ///
    /// # fn fill_buffer(buf: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// let bytes_sent = stream.send_with_timeout(&buffer, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn send_with_timeout(
        &mut self,
        buf: &impl FixedBuffer,
        timeout: Duration,
    ) -> impl Future<Output = Result<u32>> {
        self.send_with_deadline(
            buf,
            local_executor().start_round_time_for_deadlines() + timeout,
        )
    }

    /// Asynchronously sends the entire provided byte slice.
    ///
    /// # Difference between `send_all` and `send_all_bytes`
    ///
    /// Use [`send_all`](Self::send_all) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// stream.send_all_bytes(data).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all_bytes(&mut self, buf: &[u8]) -> Result<()> {
        let mut sent = 0;
        while sent < buf.len() {
            sent += self.send_bytes(&buf[sent..]).await?;
        }

        Ok(())
    }

    /// Asynchronously sends the entire provided [`Buffer`].
    ///
    /// # Difference between `send_all` and `send_all_bytes`
    ///
    /// Use [`send_all`](Self::send_all) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{buffer, AsyncConnectStream, AsyncSend};
    ///
    /// # fn fill_buffer(buf: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// stream.send_all(&buffer).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all(&mut self, buf: &impl FixedBuffer) -> Result<()> {
        if buf.is_fixed() {
            let mut sent = 0;

            #[allow(
                clippy::cast_possible_wrap,
                reason = "We believe it never send u32::MAX bytes"
            )]
            while sent < buf.len_u32() {
                sent += SendFixed::new(
                    AsRawSocket::as_raw_socket(self),
                    unsafe { buf.as_ptr().offset(sent as isize) },
                    buf.len_u32() - sent,
                    buf.fixed_index(),
                )
                .await?;
            }
        } else {
            let mut sent = 0;
            let slice = buf.as_bytes();

            while sent < slice.len() {
                sent += self.send_bytes(&slice[sent..]).await?;
            }
        }

        Ok(())
    }

    /// Asynchronously sends the entire provided byte slice with a specified deadline.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `send_all_with_deadline` and `send_all_bytes_with_deadline`
    ///
    /// Use [`send_all_with_deadline`](Self::send_all_with_deadline) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    /// use std::time::{Duration, Instant};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    ///
    /// stream.send_all_bytes_with_deadline(b"Hello, World!", deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all_bytes_with_deadline(&mut self, buf: &[u8], deadline: Instant) -> Result<()> {
        let mut sent = 0;

        while sent < buf.len() {
            sent += self
                .send_bytes_with_deadline(&buf[sent..], deadline)
                .await?;
        }

        Ok(())
    }

    /// Asynchronously sends the entire provided [`Buffer`] with a specified deadline.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `send_all_with_deadline` and `send_all_bytes_with_deadline`
    ///
    /// Use [`send_all_with_deadline`](Self::send_all_with_deadline) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{buffer, AsyncConnectStream, AsyncSend};
    /// use std::time::{Duration, Instant};
    ///
    /// # fn fill_buffer(buf: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// stream.send_all_with_deadline(&buffer, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all_with_deadline(
        &mut self,
        buf: &impl FixedBuffer,
        deadline: Instant,
    ) -> Result<()> {
        if buf.is_fixed() {
            let mut sent = 0;

            #[allow(
                clippy::cast_possible_wrap,
                reason = "We believe it never send u32::MAX bytes"
            )]
            while sent < buf.len_u32() {
                sent += SendFixedWithDeadline::new(
                    AsRawSocket::as_raw_socket(self),
                    unsafe { buf.as_ptr().offset(sent as isize) },
                    buf.len_u32() - sent,
                    buf.fixed_index(),
                    deadline,
                )
                .await?;
            }
        } else {
            let mut sent = 0;
            let slice = buf.as_bytes();

            while sent < slice.len() {
                sent += self
                    .send_bytes_with_deadline(&slice[sent..], deadline)
                    .await?;
            }
        }

        Ok(())
    }

    /// Asynchronously sends the entire provided byte slice with a specified timeout.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `send_all_with_timeout` and `send_all_bytes_with_timeout`
    ///
    /// Use [`send_all_with_timeout`](Self::send_all_with_timeout) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    /// use std::time::Duration;
    ///
    /// # fn fill_buffer(buf: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let timeout = Duration::from_secs(5);
    /// stream.send_all_bytes_with_timeout(data, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn send_all_bytes_with_timeout(
        &mut self,
        buf: &[u8],
        timeout: Duration,
    ) -> impl Future<Output = Result<()>> {
        self.send_all_bytes_with_deadline(
            buf,
            local_executor().start_round_time_for_deadlines() + timeout,
        )
    }

    /// Asynchronously sends the entire provided [`Buffer`] with a specified timeout.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `send_all_with_timeout` and `send_all_bytes_with_timeout`
    ///
    /// Use [`send_all_with_timeout`](Self::send_all_with_timeout) if it is possible,
    /// because [`Buffer`] can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{buffer, AsyncConnectStream, AsyncSend};
    /// use std::time::Duration;
    ///
    /// # fn fill_buffer(buf: &mut orengine::io::Buffer) {}
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// let mut buffer = buffer();
    ///
    /// fill_buffer(&mut buffer);
    ///
    /// stream.send_all_bytes_with_timeout(&buffer, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn send_all_with_timeout(
        &mut self,
        buf: &impl FixedBuffer,
        timeout: Duration,
    ) -> impl Future<Output = Result<()>> {
        self.send_all_with_deadline(
            buf,
            local_executor().start_round_time_for_deadlines() + timeout,
        )
    }
}

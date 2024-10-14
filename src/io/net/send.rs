use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};

/// `send` io operation.
pub struct Send<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> Send<'buf> {
    /// Creates new `send` io operation.
    pub fn new(fd: RawFd, buf: &'buf [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request_data: None,
        }
    }
}

impl<'buf> Future for Send<'buf> {
    type Output = Result<usize>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.send(this.fd, this.buf.as_ptr(), this.buf.len(), unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ret
        ));
    }
}

/// `send` io operation with deadline.
pub struct SendWithDeadline<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
}

impl<'buf> SendWithDeadline<'buf> {
    /// Creates new `send` io operation with deadline.
    pub fn new(fd: RawFd, buf: &'buf [u8], deadline: Instant) -> Self {
        Self {
            fd,
            buf,
            io_request_data: None,
            deadline,
        }
    }
}

impl<'buf> Future for SendWithDeadline<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.send_with_deadline(
                this.fd,
                this.buf.as_ptr(),
                this.buf.len(),
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() },
                &mut this.deadline
            ),
            ret
        ));
    }
}

/// The `AsyncSend` trait provides asynchronous methods for sending data over a stream or socket.
/// It allows for sending data with or without deadlines, and ensures the complete transmission
/// of data when required.
///
/// This trait can be implemented for any sockets that supports the `AsRawFd` trait
/// and can be connected.
///
/// # Example
///
/// ```no_run
/// use orengine::net::TcpStream;
/// use orengine::io::{AsyncConnectStream, AsyncSend};
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
/// let data = b"Hello, World!";
///
/// // Send data over the stream
/// let bytes_sent = stream.send(data).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncSend: AsRawFd {
    /// Asynchronously sends data. Returns the number of bytes sent.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let bytes_sent = stream.send(data).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send(&mut self, buf: &[u8]) -> Result<usize> {
        Send::new(self.as_raw_fd(), buf).await
    }

    /// Asynchronously sends data with a specified deadline.
    /// Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    /// use std::time::{Duration, Instant};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// let bytes_sent = stream.send_with_deadline(data, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_with_deadline(&mut self, buf: &[u8], deadline: Instant) -> Result<usize> {
        SendWithDeadline::new(self.as_raw_fd(), buf, deadline).await
    }

    /// Asynchronously sends data with a specified timeout. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let timeout = Duration::from_secs(5);
    /// let bytes_sent = stream.send_with_timeout(data, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_with_timeout(&mut self, buf: &[u8], timeout: Duration) -> Result<usize> {
        SendWithDeadline::new(self.as_raw_fd(), buf, Instant::now() + timeout).await
    }

    /// Asynchronously sends all the data in the buffer, ensuring that the
    /// entire buffer is transmitted.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// stream.send_all(data).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all(&mut self, buf: &[u8]) -> Result<()> {
        let mut sent = 0;
        while sent < buf.len() {
            sent += self.send(&buf[sent..]).await?;
        }
        Ok(())
    }

    /// Asynchronously sends all the data in the buffer with a specified deadline.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    /// use std::time::{Duration, Instant};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// stream.send_all_with_deadline(data, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all_with_deadline(&mut self, buf: &[u8], deadline: Instant) -> Result<()> {
        let mut sent = 0;
        while sent < buf.len() {
            sent += self.send_with_deadline(&buf[sent..], deadline).await?;
        }
        Ok(())
    }

    /// Asynchronously sends all the data in the buffer with a specified timeout.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncSend};
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let data = b"Hello, World!";
    /// let timeout = Duration::from_secs(5);
    /// stream.send_all_with_timeout(data, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all_with_timeout(&mut self, buf: &[u8], timeout: Duration) -> Result<()> {
        self.send_all_with_deadline(buf, Instant::now() + timeout)
            .await
    }
}

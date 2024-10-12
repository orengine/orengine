use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};

/// `peek` io operation.
pub struct Peek<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
}

impl<'buf> Peek<'buf> {
    /// Creates a new `peek` io operation.
    pub fn new(fd: RawFd, buf: &'buf mut [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request_data: None,
        }
    }
}

impl<'buf> Future for Peek<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.peek(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len(),
                this.io_request_data.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

/// `peek` io operation with deadline.
pub struct PeekWithDeadline<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
}

impl<'buf> PeekWithDeadline<'buf> {
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

impl<'buf> Future for PeekWithDeadline<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.peek_with_deadline(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len(),
                this.io_request_data.as_mut().unwrap_unchecked(),
                &mut this.deadline
            ),
            ret
        ));
    }
}

/// The `AsyncPeek` trait provides asynchronous methods for peeking at the incoming data
/// without consuming it. It offers options to peek with deadlines, timeouts, and to ensure
/// reading an exact number of bytes.
///
/// This trait can be implemented for any socket that supports the `AsRawFd` and can be connected.
///
/// # Example
///
/// ```no_run
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
    /// Asynchronously peeks into the incoming data without consuming it, filling the buffer with
    /// available data. Returns the number of bytes peeked.
    ///
    /// # Example
    ///
    /// ```no_run
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
        Peek::new(self.as_raw_fd(), buf).await
    }

    /// Asynchronously peeks into the incoming data with a specified deadline.
    /// Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
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
        PeekWithDeadline::new(self.as_raw_fd(), buf, deadline).await
    }

    /// Asynchronously peeks into the incoming data with a specified timeout.
    /// Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
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

    /// Asynchronously peeks into the incoming data until the buffer is completely filled with
    /// exactly the requested number of bytes.
    ///
    /// # Example
    ///
    /// ```no_run
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

    /// Asynchronously peeks into the incoming data with a deadline until the buffer is completely
    /// filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
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

    /// Asynchronously peeks into the incoming data with a timeout until the buffer is completely
    /// filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
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

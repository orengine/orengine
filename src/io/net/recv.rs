use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};

/// `recv` io operation.
#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Recv<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    io_request: Option<IoRequest>,
}

impl<'buf> Recv<'buf> {
    /// Creates a new `recv` io operation.
    pub fn new(fd: RawFd, buf: &'buf mut [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request: None,
        }
    }
}

impl<'buf> Future for Recv<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.recv(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len(),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

/// `recv` io operation with deadline.
#[must_use = "Future must be awaited to drive the IO operation"]
pub struct RecvWithDeadline<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'buf> RecvWithDeadline<'buf> {
    /// Creates a new `recv` io operation with deadline.
    pub fn new(fd: RawFd, buf: &'buf mut [u8], deadline: Instant) -> Self {
        Self {
            fd,
            buf,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'buf> Future for RecvWithDeadline<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.recv(
                this.fd,
                this.buf.as_mut_ptr(),
                this.buf.len(),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

/// The `AsyncRecv` trait provides asynchronous methods for receiving at the incoming data
/// with consuming it. It offers options to recv with deadlines, timeouts, and to ensure
/// reading an exact number of bytes.
///
/// This trait can be implemented for any socket that supports the `AsRawFd`
/// and can be connected.
///
/// # Example
///
/// ```no_run
/// use orengine::buf::full_buffer;
/// use orengine::net::TcpStream;
/// use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv};
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
/// stream.poll_recv().await?;
/// let mut buf = full_buffer();
///
/// // Recv at the incoming data with consuming it
/// let bytes_peeked = stream.recv(&mut buf).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncRecv: AsRawFd {
    /// Asynchronously receives into the incoming data with consuming it, filling the buffer with
    /// available data. Returns the number of bytes received.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// stream.poll_recv().await?;
    /// let mut buf = full_buffer();
    /// let bytes_peeked = stream.recv(&mut buf).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        Recv::new(self.as_raw_fd(), buf).await
    }

    /// Asynchronously receives into the incoming data with a specified deadline.
    /// Returns the number of bytes received.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::buf::full_buffer;
    /// use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv};
    /// use std::time::{Duration, Instant};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// stream.poll_recv_with_deadline(deadline).await?;
    /// let mut buf = full_buffer();
    ///
    /// let bytes_peeked = stream.recv_with_deadline(&mut buf, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_with_deadline(&mut self, buf: &mut [u8], deadline: Instant) -> Result<usize> {
        RecvWithDeadline::new(self.as_raw_fd(), buf, deadline).await
    }

    /// Asynchronously receives into the incoming data with a specified timeout.
    /// Returns the number of bytes received.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::buf::full_buffer;
    /// use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// stream.poll_recv_with_timeout(timeout).await?;
    /// let mut buf = full_buffer();
    ///
    /// let bytes_peeked = stream.recv_with_timeout(&mut buf, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_with_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        self.recv_with_deadline(buf, Instant::now() + timeout).await
    }

    /// Asynchronously receives into the incoming data until the buffer is completely filled with
    /// exactly the requested number of bytes.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// stream.poll_recv().await?;
    /// let mut buf = full_buffer();
    ///
    /// stream.recv_exact(&mut buf[..100]).await?; // Receive 100 bytes
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut received = 0;

        while received < buf.len() {
            received += self.recv(&mut buf[received..]).await?;
        }
        Ok(())
    }

    /// Asynchronously receives into the incoming data with a deadline
    /// until the buffer is completely filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv};
    /// use orengine::buf::full_buffer;
    /// use std::time::{Instant, Duration};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// stream.poll_recv_with_deadline(deadline).await?;
    /// let mut buf = full_buffer();
    ///
    /// stream.recv_exact_with_deadline(&mut buf[..100], deadline).await?; // Receive 100 bytes
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_exact_with_deadline(&mut self, buf: &mut [u8], deadline: Instant) -> Result<()> {
        let mut received = 0;

        while received < buf.len() {
            received += self.recv_with_deadline(&mut buf[received..], deadline).await?;
        }
        Ok(())
    }

    /// Asynchronously receives into the incoming data with a timeout until the buffer is completely
    /// filled with the exact number of bytes.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv};
    /// use orengine::buf::full_buffer;
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// stream.poll_recv_with_timeout(timeout).await?;
    /// let mut buf = full_buffer();
    ///
    /// stream.recv_exact_with_timeout(&mut buf[..100], timeout).await?; // Receive 100 bytes
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_exact_with_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()> {
        self.recv_exact_with_deadline(buf, Instant::now() + timeout).await
    }
}

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

macro_rules! generate_poll {
    ($name:ident, $name_with_deadline:ident, $method:expr, $method_with_deadline:expr) => {
        /// `poll_fd` io operation.
        pub struct $name {
            fd: RawFd,
            io_request_data: Option<IoRequestData>,
        }

        impl $name {
            /// Creates a new `poll_fd` io operation.
            pub fn new(fd: RawFd) -> Self {
                Self {
                    fd,
                    io_request_data: None,
                }
            }
        }

        impl Future for $name {
            type Output = std::io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
                let ret;

                poll_for_io_request!((
                    local_worker().$method(this.fd, unsafe {
                        this.io_request_data.as_mut().unwrap_unchecked()
                    }),
                    ()
                ));
            }
        }

        unsafe impl Send for $name {}

        /// `poll_fd` io operation with deadline.
        pub struct $name_with_deadline {
            fd: RawFd,
            io_request_data: Option<IoRequestData>,
            deadline: Instant,
        }

        impl $name_with_deadline {
            /// Creates a new `poll_fd` io operation with deadline.
            pub fn new(fd: RawFd, deadline: Instant) -> Self {
                Self {
                    fd,
                    io_request_data: None,
                    deadline,
                }
            }
        }

        impl Future for $name_with_deadline {
            type Output = std::io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                let worker = local_worker();
                #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
                let ret;

                poll_for_time_bounded_io_request!((
                    worker.$method_with_deadline(
                        this.fd,
                        unsafe { this.io_request_data.as_mut().unwrap_unchecked() },
                        &mut this.deadline
                    ),
                    ()
                ));
            }
        }

        unsafe impl Send for $name_with_deadline {}
    };
}

generate_poll!(
    PollRecv,
    PollRecvWithDeadline,
    poll_fd_read,
    poll_fd_read_with_deadline
);
generate_poll!(
    PollSend,
    PollSendWithDeadline,
    poll_fd_write,
    poll_fd_write_with_deadline
);

/// The `AsyncPollFd` trait provides non-blocking polling methods for readiness in receiving
/// and sending data on file descriptors.
///
/// It enables polling with deadlines, timeouts,
/// and simple polling for both read and write readiness.
///
/// This trait can be implemented for any writable and readable structs
/// that supports the [`AsRawFd`] trait.
pub trait AsyncPollFd: AsRawFd {
    /// Returns future that will be resolved when the file descriptor
    /// becomes readable or an error occurs.
    ///
    /// # Usage
    ///
    /// Call this method on the stream before allocate a [`buffer`](crate::io::Buffer)
    /// and receive from the stream. After receive release (drop) the [`buffer`](crate::io::Buffer).
    ///
    /// Asynchronously peeks into the incoming data without consuming it, filling the buffer with
    /// available data. Returns the number of bytes peeked.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPollFd, AsyncRecv};
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
    fn poll_recv(&self) -> PollRecv {
        PollRecv::new(self.as_raw_fd())
    }

    /// Returns future that will be resolved when the file descriptor
    /// becomes readable or an error occurs or the deadline is reached.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Usage
    ///
    /// Call this method on the stream before allocate a [`buffer`](crate::io::Buffer)
    /// and receive from the stream. After receive release (drop) the [`buffer`](crate::io::Buffer).
    ///
    /// Asynchronously peeks into the incoming data with a specified deadline.
    /// Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPollFd, AsyncRecv};
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
    fn poll_recv_with_deadline(&self, deadline: Instant) -> PollRecvWithDeadline {
        PollRecvWithDeadline::new(self.as_raw_fd(), deadline)
    }

    /// Returns future that will be resolved when the file descriptor
    /// becomes readable or an error occurs or the timeout is exceeded.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Usage
    ///
    /// Call this method on the stream before allocate a [`buffer`](crate::io::Buffer)
    /// and receive from the stream. After receive release (drop) the [`buffer`](crate::io::Buffer).
    ///
    /// Asynchronously peeks into the incoming data with a specified timeout.
    /// Returns the number of bytes peeked.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::{full_buffer, AsyncConnectStream, AsyncPollFd, AsyncRecv};
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
    fn poll_recv_with_timeout(&self, timeout: Duration) -> PollRecvWithDeadline {
        self.poll_recv_with_deadline(Instant::now() + timeout)
    }

    /// Returns future that will be resolved when the file descriptor
    /// becomes writable or an error occurs.
    ///
    /// # Usage
    ///
    /// Call this method on the stream before allocate a [`buffer`](crate::io::Buffer)
    /// and send to the stream. After send release (drop) the [`buffer`](crate::io::Buffer).
    /// As opposed to [`poll_recv`](Self::poll_recv) it does not have a significant impact
    /// on productivity and efficiency.
    #[inline(always)]
    fn poll_send(&self) -> PollSend {
        PollSend::new(self.as_raw_fd())
    }

    /// Returns future that will be resolved when the file descriptor
    /// becomes writable or an error occurs or the deadline is reached.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Usage
    ///
    /// Call this method on the stream before allocate a [`buffer`](crate::io::Buffer)
    /// and send to the stream. After send release (drop) the [`buffer`](crate::io::Buffer).
    /// As opposed to [`poll_recv_with_deadline`](Self::poll_recv_with_deadline) it does not have a significant impact
    /// on productivity and efficiency.
    #[inline(always)]
    fn poll_send_with_deadline(&self, deadline: Instant) -> PollSendWithDeadline {
        PollSendWithDeadline::new(self.as_raw_fd(), deadline)
    }

    /// Returns future that will be resolved when the file descriptor
    /// becomes writable or an error occurs or the timeout is exceeded.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Usage
    ///
    /// Call this method on the stream before allocate a [`buffer`](crate::io::Buffer)
    /// and send to the stream. After send release (drop) the [`buffer`](crate::io::Buffer).
    /// As opposed to [`poll_recv_with_timeout`](Self::poll_recv_with_timeout) it does not have a significant impact
    /// on productivity and efficiency.
    #[inline(always)]
    fn poll_send_with_timeout(&self, timeout: Duration) -> PollSendWithDeadline {
        self.poll_send_with_deadline(Instant::now() + timeout)
    }
}

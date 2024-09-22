use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use crate::io::io_request::{IoRequest};
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{IoWorker, local_worker};
use crate::io::io_sleeping_task::TimeBoundedIoTask;

macro_rules! generate_poll {
    ($name: ident, $name_with_deadline: ident, $method: expr) => {
        /// `poll_fd` io operation.
        #[must_use = "Future must be awaited to drive the IO operation"]
        pub struct $name {
            fd: RawFd,
            io_request: Option<IoRequest>
        }

        impl $name {
            /// Creates a new `poll_fd` io operation.
            pub fn new(fd: RawFd) -> Self {
                Self {
                    fd,
                    io_request: None
                }
            }
        }

        impl Future for $name {
            type Output = std::io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                let worker = unsafe { local_worker() };
                #[allow(unused)]
                let ret;

                poll_for_io_request!((
                     worker.$method(this.fd, this.io_request.as_mut().unwrap_unchecked()),
                     ()
                ));
            }
        }

        /// `poll_fd` io operation with deadline.
        #[must_use = "Future must be awaited to drive the IO operation"]
        pub struct $name_with_deadline {
            fd: RawFd,
            time_bounded_io_task: TimeBoundedIoTask,
            io_request: Option<IoRequest>
        }

        impl $name_with_deadline {
            /// Creates a new `poll_fd` io operation with deadline.
            pub fn new(fd: RawFd, deadline: Instant) -> Self {
                Self {
                    fd,
                    time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
                    io_request: None
                }
            }
        }

        impl Future for $name_with_deadline {
            type Output = std::io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                let worker = unsafe { local_worker() };
                #[allow(unused)]
                let ret;

                poll_for_time_bounded_io_request!((
                     worker.$method(this.fd, this.io_request.as_mut().unwrap_unchecked()),
                     ()
                ));
            }
        }
    }
}

generate_poll!(PollRecv, PollRecvWithDeadline, poll_fd_read);
generate_poll!(PollSend, PollSendWithDeadline, poll_fd_write);

/// The AsyncPollFd trait provides non-blocking polling methods for readiness in receiving
/// and sending data on file descriptors. It enables polling with deadlines, timeouts,
/// and simple polling for both read and write readiness.
///
/// This trait can be implemented for any writable and readable structs
/// that supports the AsRawFd trait.
pub trait AsyncPollFd: AsRawFd {
    /// Returns future that will be resolved when the file descriptor
    /// becomes readable or an error occurs.
    ///
    /// # Usage
    ///
    /// Call this method on the stream before allocate a [`buffer`](crate::buf::Buffer)
    /// and receive from the stream. After receive release (drop) the [`buffer`](crate::buf::Buffer).
    ///
    /// Asynchronously peeks into the incoming data without consuming it, filling the buffer with
    /// available data. Returns the number of bytes peeked.
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
    /// Call this method on the stream before allocate a [`buffer`](crate::buf::Buffer)
    /// and receive from the stream. After receive release (drop) the [`buffer`](crate::buf::Buffer).
    ///
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
    /// Call this method on the stream before allocate a [`buffer`](crate::buf::Buffer)
    /// and receive from the stream. After receive release (drop) the [`buffer`](crate::buf::Buffer).
    ///
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
    fn poll_recv_with_timeout(&self, timeout: Duration) -> PollRecvWithDeadline {
        self.poll_recv_with_deadline(Instant::now() + timeout)
    }

    /// Returns future that will be resolved when the file descriptor
    /// becomes writable or an error occurs.
    ///
    /// # Usage
    ///
    /// Call this method on the stream before allocate a [`buffer`](crate::buf::Buffer)
    /// and send to the stream. After send release (drop) the [`buffer`](crate::buf::Buffer).
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
    /// Call this method on the stream before allocate a [`buffer`](crate::buf::Buffer)
    /// and send to the stream. After send release (drop) the [`buffer`](crate::buf::Buffer).
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
    /// Call this method on the stream before allocate a [`buffer`](crate::buf::Buffer)
    /// and send to the stream. After send release (drop) the [`buffer`](crate::buf::Buffer).
    /// As opposed to [`poll_recv_with_timeout`](Self::poll_recv_with_timeout) it does not have a significant impact
    /// on productivity and efficiency.
    #[inline(always)]
    fn poll_send_with_timeout(&self, timeout: Duration) -> PollSendWithDeadline {
        self.poll_send_with_deadline(Instant::now() + timeout)
    }
}
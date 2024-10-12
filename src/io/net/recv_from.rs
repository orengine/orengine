use std::future::Future;
use std::io::Result;
use std::mem;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, MessageRecvHeader, RawFd};
use crate::io::worker::{local_worker, IoWorker};
use crate::BUG_MESSAGE;

/// `recv_from` io operation.
pub struct RecvFrom<'fut> {
    fd: RawFd,
    msg_header: MessageRecvHeader<'fut>,
    buf: &'fut mut [u8],
    addr: &'fut mut SockAddr,
    io_request_data: Option<IoRequestData>,
}

impl<'fut> RecvFrom<'fut> {
    /// Creates a new `recv_from` io operation.
    pub fn new(fd: RawFd, buf: &'fut mut [u8], addr: &'fut mut SockAddr) -> Self {
        Self {
            fd,
            msg_header: MessageRecvHeader::new(),
            buf,
            addr,
            io_request_data: None,
        }
    }
}

impl<'fut> Future for RecvFrom<'fut> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.recv_from(
                this.fd,
                this.msg_header
                    .get_os_message_header_ptr(this.addr, &mut (this.buf as *mut _)),
                this.io_request_data.as_mut().unwrap_unchecked()
            ),
            ret
        ));
    }
}

/// `recv_from` io operation with deadline.
pub struct RecvFromWithDeadline<'fut> {
    fd: RawFd,
    msg_header: MessageRecvHeader<'fut>,
    buf: &'fut mut [u8],
    addr: &'fut mut SockAddr,
    deadline: Instant,
    io_request_data: Option<IoRequestData>,
}

impl<'fut> RecvFromWithDeadline<'fut> {
    /// Creates a new `recv_from` io operation with deadline.
    pub fn new(
        fd: RawFd,
        buf: &'fut mut [u8],
        addr: &'fut mut SockAddr,
        deadline: Instant,
    ) -> Self {
        Self {
            fd,
            msg_header: MessageRecvHeader::new(),
            buf,
            addr,
            deadline,
            io_request_data: None,
        }
    }
}

impl<'fut> Future for RecvFromWithDeadline<'fut> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.recv_from_with_deadline(
                this.fd,
                this.msg_header
                    .get_os_message_header_ptr(this.addr, &mut (this.buf as *mut _)),
                this.io_request_data.as_mut().unwrap_unchecked(),
                &mut this.deadline
            ),
            ret
        ));
    }
}

/// The `AsyncRecvFrom` trait provides asynchronous methods for receiving at the incoming data
/// with consuming it from the socket (datagram).
/// It returns [`SocketAddr`](SocketAddr) of the sender.
/// It offers options to peek with deadlines, timeouts, and to ensure
/// reading an exact number of bytes.
///
/// This trait can be implemented for any datagram-oriented socket that supports the `AsRawFd`.
///
/// # Example
///
/// ```no_run
/// use orengine::buf::full_buffer;
/// use orengine::io::{AsyncBind, AsyncPeekFrom, AsyncPollFd, AsyncRecvFrom};
/// use orengine::net::UdpSocket;
///
/// async fn foo() -> std::io::Result<()> {
/// let mut stream = UdpSocket::bind("127.0.0.1:8080").await?;
/// stream.poll_recv().await?;
/// let mut buf = full_buffer();
///
/// // Receive at the incoming data with consuming it
/// let bytes_peeked = stream.recv_from(&mut buf).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncRecvFrom: AsRawFd {
    /// Asynchronously receives into the incoming datagram with consuming it, filling the buffer
    /// with available data and returning the number of bytes received and the sender's address.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::io::{AsyncBind, AsyncPollFd, AsyncRecvFrom};
    /// use orengine::net::UdpSocket;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("127.0.0.1:8080").await?;
    /// socket.poll_recv().await?;
    /// let mut buf = full_buffer();
    ///
    /// // Receive at the incoming datagram without consuming it
    /// let (bytes_peeked, addr) = socket.recv_from(&mut buf).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_from(&mut self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        let mut sock_addr = unsafe { mem::zeroed() };
        let n = RecvFrom::new(self.as_raw_fd(), buf, &mut sock_addr).await?;

        Ok((n, sock_addr.as_socket().expect(BUG_MESSAGE)))
    }

    /// Asynchronously receives into the incoming datagram with a deadline, with consuming it,
    /// filling the buffer with available data and returning the number of bytes received
    /// and the sender's address.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::io::{AsyncBind, AsyncPollFd, AsyncRecvFrom};
    /// use orengine::net::UdpSocket;
    /// use std::time::{Duration, Instant};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// socket.poll_recv_with_deadline(deadline).await?;
    /// let mut buf = full_buffer();
    ///
    /// // Receive at the incoming datagram with a deadline
    /// let (bytes_peeked, addr) = socket.recv_from_with_deadline(&mut buf, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_from_with_deadline(
        &mut self,
        buf: &mut [u8],
        deadline: Instant,
    ) -> Result<(usize, SocketAddr)> {
        let mut sock_addr = unsafe { mem::zeroed() };
        let n = RecvFromWithDeadline::new(self.as_raw_fd(), buf, &mut sock_addr, deadline).await?;

        Ok((n, sock_addr.as_socket().expect(BUG_MESSAGE)))
    }

    /// Asynchronously receives into the incoming datagram with a timeout, with consuming it,
    /// filling the buffer with available data and returning the number of bytes received
    /// and the sender's address.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::buf::full_buffer;
    /// use orengine::io::{AsyncBind, AsyncPollFd, AsyncRecvFrom};
    /// use orengine::net::UdpSocket;
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// socket.poll_recv_with_timeout(timeout).await?;
    /// let mut buf = full_buffer();
    ///
    /// // Peek at the incoming datagram with a timeout
    /// let (bytes_peeked, addr) = socket.recv_from_with_timeout(&mut buf, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_from_with_timeout(
        &mut self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> Result<(usize, SocketAddr)> {
        self.recv_from_with_deadline(buf, Instant::now() + timeout)
            .await
    }
}

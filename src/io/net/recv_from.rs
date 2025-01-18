use std::future::Future;
use std::io::{IoSliceMut, Result};
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{AsRawSocket, MessageRecvHeader, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::FixedBufferMut;
use crate::net::addr::FromSockAddr;
use crate::net::Socket;
use crate::BUG_MESSAGE;

/// `recv_from` io operation.
pub struct RecvFrom<'fut> {
    raw_socket: RawSocket,
    sock_addr: &'fut mut SockAddr,
    msg_header: MessageRecvHeader,
    io_request_data: Option<IoRequestData>,
}

impl<'fut> RecvFrom<'fut> {
    /// Creates a new `recv_from` io operation.
    pub fn new(
        raw_socket: RawSocket,
        buf_ptr: *mut [IoSliceMut],
        addr: &'fut mut SockAddr,
    ) -> Self {
        Self {
            raw_socket,
            msg_header: MessageRecvHeader::new(addr, buf_ptr),
            sock_addr: addr,
            io_request_data: None,
        }
    }
}

impl Future for RecvFrom<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().recv_from(this.raw_socket, &mut this.msg_header, unsafe {
                IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked())
            }),
            {
                unsafe { this.sock_addr.set_length(this.msg_header.get_addr_len()) };
                ret
            }
        ));
    }
}

#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "We guarantee that `RecvFrom` is `Send`."
)]
unsafe impl Send for RecvFrom<'_> {}

/// `recv_from` io operation with deadline.
pub struct RecvFromWithDeadline<'fut> {
    raw_socket: RawSocket,
    sock_addr: &'fut mut SockAddr,
    msg_header: MessageRecvHeader,
    deadline: Instant,
    io_request_data: Option<IoRequestData>,
}

impl<'fut> RecvFromWithDeadline<'fut> {
    /// Creates a new `recv_from` io operation with deadline.
    pub fn new(
        raw_socket: RawSocket,
        buf_ptr: *mut [IoSliceMut],
        addr: &'fut mut SockAddr,
        deadline: Instant,
    ) -> Self {
        Self {
            raw_socket,
            msg_header: MessageRecvHeader::new(addr, buf_ptr),
            sock_addr: addr,
            deadline,
            io_request_data: None,
        }
    }
}

impl Future for RecvFromWithDeadline<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.recv_from_with_deadline(
                this.raw_socket,
                &mut this.msg_header,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) },
                &mut this.deadline
            ),
            {
                unsafe { this.sock_addr.set_length(this.msg_header.get_addr_len()) };
                ret
            }
        ));
    }
}

#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "We guarantee that `RecvFromWithDeadline` is `Send`."
)]
unsafe impl Send for RecvFromWithDeadline<'_> {}

/// The `AsyncRecvFrom` trait provides asynchronous methods for receiving at the incoming data
/// with consuming it from the socket (datagram).
///
/// It returns [`SocketAddr`](std::net::SocketAddr) of the sender.
/// It offers options to peek with deadlines, timeouts, and to ensure
/// reading an exact number of bytes.
///
/// This trait can be implemented for any datagram-oriented [`socket`](Socket).
///
/// # Example
///
/// ```rust
/// use orengine::io::{full_buffer, AsyncBind, AsyncPeekFrom, AsyncPollSocket, AsyncRecvFrom};
/// use orengine::net::UdpSocket;
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut stream = UdpSocket::bind("127.0.0.1:8080").await?;
/// stream.poll_recv().await?;
/// let mut buf = full_buffer();
///
/// // Receive at the incoming data with consuming it
/// let bytes_peeked = stream.recv_from(&mut buf).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncRecvFrom: Socket {
    /// Asynchronously receives into the incoming datagram with consuming it, filling the buffer
    /// with available data and returning the number of bytes received and the sender's address.
    ///
    /// # Difference between `recv_bytes_from` and `recv_from`
    ///
    /// Use `recv_from` if it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::{full_buffer, AsyncBind, AsyncPollSocket, AsyncRecvFrom};
    /// use orengine::net::UdpSocket;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("127.0.0.1:8080").await?;
    /// socket.poll_recv().await?;
    /// let mut buf = full_buffer();
    ///
    /// // Receive at the incoming datagram without consuming it
    /// let (bytes_peeked, addr) = socket.recv_bytes_from(&mut buf).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_bytes_from(&mut self, buf: &mut [u8]) -> Result<(usize, Self::Addr)> {
        let mut sock_addr = unsafe { mem::zeroed() };
        let buf_ptr = &mut [IoSliceMut::new(buf)];

        let n = RecvFrom::new(AsRawSocket::as_raw_socket(self), buf_ptr, &mut sock_addr).await?;

        Ok((n, Self::Addr::from_sock_addr(sock_addr).expect(BUG_MESSAGE)))
    }

    /// Asynchronously receives into the incoming datagram with consuming it, filling the buffer
    /// with available data and returning the number of bytes received and the sender's address.
    ///
    /// # Difference between `recv_from` and `recv_bytes_from`
    ///
    /// Use `recv_from` if it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::{full_buffer, AsyncBind, AsyncPollSocket, AsyncRecvFrom};
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
    async fn recv_from(&mut self, buf: &mut impl FixedBufferMut) -> Result<(usize, Self::Addr)> {
        // Now RecvFrom with `fixed` buffer is unsupported.
        self.recv_bytes_from(buf.as_bytes_mut()).await
    }

    /// Asynchronously receives into the incoming datagram with a deadline, with consuming it,
    /// filling the buffer with available data and returning the number of bytes received
    /// and the sender's address.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_bytes_from_with_deadline` and `recv_from_with_deadline`
    ///
    /// Use `recv_from_with_deadline` if it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::{full_buffer, AsyncBind, AsyncPollSocket, AsyncRecvFrom};
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
    /// let (bytes_peeked, addr) = socket.recv_bytes_from_with_deadline(&mut buf, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_bytes_from_with_deadline(
        &mut self,
        buf: &mut [u8],
        deadline: Instant,
    ) -> Result<(usize, Self::Addr)> {
        let mut sock_addr = unsafe { mem::zeroed() };
        let buf_ptr = &mut [IoSliceMut::new(buf)];

        let n = RecvFromWithDeadline::new(
            AsRawSocket::as_raw_socket(self),
            buf_ptr,
            &mut sock_addr,
            deadline,
        )
        .await?;

        Ok((n, Self::Addr::from_sock_addr(sock_addr).expect(BUG_MESSAGE)))
    }

    /// Asynchronously receives into the incoming datagram with a deadline, with consuming it,
    /// filling the buffer with available data and returning the number of bytes received
    /// and the sender's address.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_from_with_deadline` and `recv_bytes_from_with_deadline`
    ///
    /// Use `recv_from_with_deadline` if it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::{full_buffer, AsyncBind, AsyncPollSocket, AsyncRecvFrom};
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
        buf: &mut impl FixedBufferMut,
        deadline: Instant,
    ) -> Result<(usize, Self::Addr)> {
        // Now RecvFrom with `fixed` buffer is unsupported.
        self.recv_bytes_from_with_deadline(buf.as_bytes_mut(), deadline)
            .await
    }

    /// Asynchronously receives into the incoming datagram with a timeout, with consuming it,
    /// filling the buffer with available data and returning the number of bytes received
    /// and the sender's address.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_bytes_from_with_timeout` and `recv_from_with_timeout`
    ///
    /// Use `recv_from_with_timeout` if it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::{full_buffer, AsyncBind, AsyncPollSocket, AsyncRecvFrom};
    /// use orengine::net::UdpSocket;
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// socket.poll_recv_with_timeout(timeout).await?;
    /// let mut buf = full_buffer();
    ///
    /// // Receive at the incoming datagram with a timeout
    /// let (bytes_peeked, addr) = socket.recv_bytes_from_with_timeout(&mut buf, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_bytes_from_with_timeout(
        &mut self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> Result<(usize, Self::Addr)> {
        self.recv_bytes_from_with_deadline(buf, std::time::Instant::now() + timeout)
            .await
    }

    /// Asynchronously receives into the incoming datagram with a timeout, with consuming it,
    /// filling the buffer with available data and returning the number of bytes received
    /// and the sender's address.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Difference between `recv_from_with_timeout` and `recv_bytes_from_with_timeout`
    ///
    /// Use `recv_from_with_timeout` if it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::{full_buffer, AsyncBind, AsyncPollSocket, AsyncRecvFrom};
    /// use orengine::net::UdpSocket;
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("127.0.0.1:8080").await?;
    /// let timeout = Duration::from_secs(5);
    /// socket.poll_recv_with_timeout(timeout).await?;
    /// let mut buf = full_buffer();
    ///
    /// // Receive at the incoming datagram with a timeout
    /// let (bytes_peeked, addr) = socket.recv_from_with_timeout(&mut buf, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn recv_from_with_timeout(
        &mut self,
        buf: &mut impl FixedBufferMut,
        timeout: Duration,
    ) -> Result<(usize, Self::Addr)> {
        // Now RecvFrom with `fixed` buffer is unsupported.
        self.recv_bytes_from_with_timeout(buf.as_bytes_mut(), timeout)
            .await
    }
}

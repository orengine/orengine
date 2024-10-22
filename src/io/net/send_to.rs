use std::future::Future;
use std::io;
use std::io::{ErrorKind, Result};
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, MessageSendHeader, RawFd};
use crate::io::worker::{local_worker, IoWorker};

/// `send_to` io operation.
pub struct SendTo<'fut> {
    fd: RawFd,
    message_header: MessageSendHeader<'fut>,
    buf: &'fut [u8],
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>,
}

impl<'fut> SendTo<'fut> {
    /// Creates a new `send_to` io operation.
    pub fn new(fd: RawFd, buf: &'fut [u8], addr: &'fut SockAddr) -> Self {
        Self {
            fd,
            message_header: MessageSendHeader::new(),
            buf,
            addr,
            io_request_data: None,
        }
    }
}

impl<'fut> Future for SendTo<'fut> {
    type Output = Result<usize>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_io_request!((
            worker.send_to(
                this.fd,
                this.message_header
                    .get_os_message_header_ptr(this.addr, &mut (this.buf as *const _)),
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            ret
        ));
    }
}

unsafe impl Send for SendTo<'_> {}

/// `send_to` io operation with deadline.
pub struct SendToWithDeadline<'fut> {
    fd: RawFd,
    message_header: MessageSendHeader<'fut>,
    buf: &'fut [u8],
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
}

impl<'fut> SendToWithDeadline<'fut> {
    /// Creates a new `send_to` io operation with deadline.
    pub fn new(fd: RawFd, buf: &'fut [u8], addr: &'fut SockAddr, deadline: Instant) -> Self {
        Self {
            fd,
            message_header: MessageSendHeader::new(),
            buf,
            addr,
            io_request_data: None,
            deadline,
        }
    }
}

impl<'fut> Future for SendToWithDeadline<'fut> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.send_to_with_deadline(
                this.fd,
                this.message_header
                    .get_os_message_header_ptr(this.addr, &mut (this.buf as *const _)),
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() },
                &mut this.deadline
            ),
            ret
        ));
    }
}

unsafe impl Send for SendToWithDeadline<'_> {}

#[inline(always)]
/// Returns first resolved address from `ToSocketAddrs`.
fn sock_addr_from_to_socket_addr<A: ToSocketAddrs>(to_addr: A) -> Result<SockAddr> {
    loop {
        match to_addr.to_socket_addrs()?.next() {
            Some(addr) => return Ok(SockAddr::from(addr)),
            None => {}
        }

        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "no addresses to send data to",
        ));
    }
}

/// The `AsyncSendTo` trait provides asynchronous methods for sending data to a specified address
/// over a socket. It supports sending to a single address and can be done with or without deadlines
/// or timeouts. The trait can be implemented for datagram-oriented sockets
/// that implement the `AsRawFd` trait.
///
/// **Note:** The methods only send data to the first resolved address when provided with multiple
/// addresses via the `ToSocketAddrs` trait. If no address is resolved, an error is returned.
///
/// # Example
///
/// ```no_run
/// use orengine::net::UdpSocket;
/// use orengine::io::{AsyncBind, AsyncSendTo};
/// use std::net::SocketAddr;
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
///
/// // Send data to the specified address
/// socket.send_to(b"Hello, World!", "127.0.0.1:8080").await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncSendTo: AsRawFd {
    /// Asynchronously sends data to the specified address. The method only sends to the first
    /// address resolved from the `ToSocketAddrs` input. Returns the number of bytes sent.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// socket.send_to(b"Hello, World!", "127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_to<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A) -> Result<usize> {
        SendTo::new(self.as_raw_fd(), buf, &sock_addr_from_to_socket_addr(addr)?).await
    }

    /// Asynchronously sends data to the specified address with a deadline. Only the first resolved
    /// address is used. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    /// use std::time::{Duration, Instant};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let data = b"Hello, World!";
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// socket.send_to_with_deadline(data, addr, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_to_with_deadline<A: ToSocketAddrs>(
        &mut self,
        buf: &[u8],
        addr: A,
        deadline: Instant,
    ) -> Result<usize> {
        SendToWithDeadline::new(
            self.as_raw_fd(),
            buf,
            &sock_addr_from_to_socket_addr(addr)?,
            deadline,
        )
        .await
    }

    /// Asynchronously sends data to the specified address with a timeout.
    /// Only the first resolved address is used. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let data = b"Hello, World!";
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let timeout = Duration::from_secs(5);
    /// socket.send_to_with_timeout(data, addr, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_to_with_timeout<A: ToSocketAddrs>(
        &mut self,
        buf: &[u8],
        addr: A,
        timeout: Duration,
    ) -> Result<usize> {
        SendToWithDeadline::new(
            self.as_raw_fd(),
            buf,
            &sock_addr_from_to_socket_addr(addr)?,
            Instant::now() + timeout,
        )
        .await
    }

    /// Asynchronously sends all the data in the buffer to the specified address. The method
    /// ensures that the entire buffer is transmitted by repeatedly calling `send_to` until all
    /// the data is sent. Only the first resolved address is used.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let data = b"Hello, World!";
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// socket.send_all_to(data, addr).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all_to<A: ToSocketAddrs>(&mut self, buf: &[u8], addr: A) -> Result<usize> {
        let mut sent = 0;
        let addr = sock_addr_from_to_socket_addr(addr)?;

        while sent < buf.len() {
            sent += SendTo::new(self.as_raw_fd(), buf, &addr).await?;
        }

        Ok(sent)
    }

    /// Asynchronously sends all the data in the buffer to the specified address with a deadline.
    /// The method ensures that the entire buffer is transmitted before the deadline by repeatedly
    /// calling `send_to_with_deadline`. Only the first resolved address is used.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    /// use std::time::{Duration, Instant};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let data = b"Hello, World!";
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// socket.send_all_to_with_deadline(data, addr, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all_to_with_deadline<A: ToSocketAddrs>(
        &mut self,
        buf: &[u8],
        addr: A,
        deadline: Instant,
    ) -> Result<usize> {
        let mut sent = 0;
        let addr = sock_addr_from_to_socket_addr(addr)?;

        while sent < buf.len() {
            sent += SendToWithDeadline::new(self.as_raw_fd(), buf, &addr, deadline).await?;
        }

        Ok(sent)
    }

    /// Asynchronously sends all the data in the buffer to the specified address with a timeout.
    /// The method ensures that the entire buffer is transmitted before the timeout by repeatedly
    /// calling `send_to_with_timeout`. Only the first resolved address is used.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let data = b"Hello, World!";
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let timeout = Duration::from_secs(5);
    /// socket.send_all_to_with_timeout(data, addr, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn send_all_to_with_timeout<A: ToSocketAddrs>(
        &mut self,
        buf: &[u8],
        addr: A,
        timeout: Duration,
    ) -> Result<usize> {
        self.send_all_to_with_deadline(buf, addr, Instant::now() + timeout)
            .await
    }
}

use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{AsRawSocket, MessageSendHeader, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;
use std::future::Future;
use std::io::{ErrorKind, Result};
use std::marker::PhantomData;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use std::{io, ptr};

/// `send_to` io operation.
pub struct SendTo<'fut> {
    raw_socket: RawSocket,
    message_header: MessageSendHeader,
    buf: &'fut *const [u8],
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<&'fut [u8]>,
}

impl<'fut> SendTo<'fut> {
    /// Creates a new `send_to` io operation.
    pub fn new(raw_socket: RawSocket, buf: &'fut *const [u8], addr: &'fut SockAddr) -> Self {
        Self {
            raw_socket,
            message_header: MessageSendHeader::new(),
            buf,
            addr,
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl Future for SendTo<'_> {
    type Output = Result<usize>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        let os_message_header_ptr = this
            .message_header
            .get_os_message_header_ptr(this.addr, ptr::from_ref(this.buf).cast_mut());

        poll_for_io_request!((
            local_worker().send_to(this.raw_socket, os_message_header_ptr, unsafe {
                IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked())
            }),
            ret
        ));
    }
}

#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "We guarantee that `SendTo` is `Send`."
)]
unsafe impl Send for SendTo<'_> {}

/// `send_to` io operation with deadline.
pub struct SendToWithDeadline<'fut> {
    raw_socket: RawSocket,
    message_header: MessageSendHeader,
    buf: &'fut *const [u8],
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
    phantom_data: PhantomData<&'fut [u8]>,
}

impl<'fut> SendToWithDeadline<'fut> {
    /// Creates a new `send_to` io operation with deadline.
    pub fn new(
        raw_socket: RawSocket,
        buf: &'fut *const [u8],
        addr: &'fut SockAddr,
        deadline: Instant,
    ) -> Self {
        Self {
            raw_socket,
            message_header: MessageSendHeader::new(),
            buf,
            addr,
            io_request_data: None,
            deadline,
            phantom_data: PhantomData,
        }
    }
}

impl Future for SendToWithDeadline<'_> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        let os_message_header_ptr = this
            .message_header
            .get_os_message_header_ptr(this.addr, ptr::from_ref(this.buf).cast_mut());

        poll_for_time_bounded_io_request!((
            worker.send_to_with_deadline(
                this.raw_socket,
                os_message_header_ptr,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) },
                &mut this.deadline
            ),
            ret
        ));
    }
}

#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "We guarantee that `SendToWithDeadline` is `Send`."
)]
unsafe impl Send for SendToWithDeadline<'_> {}

#[inline(always)]
/// Returns first resolved address from `ToSocketAddrs`.
fn sock_addr_from_to_socket_addr<A: ToSocketAddrs>(to_addr: A) -> Result<SockAddr> {
    let mut addrs = to_addr.to_socket_addrs()?;
    if let Some(addr) = addrs.next() {
        return Ok(SockAddr::from(addr));
    }

    Err(io::Error::new(
        ErrorKind::InvalidInput,
        "no addresses to send data to",
    ))
}

/// The `AsyncSendTo` trait provides asynchronous methods for sending data to a specified address
/// over a socket.
///
/// It supports sending to a single address and can be done with or without deadlines
/// or timeouts. The trait can be implemented for datagram-oriented sockets
/// that implement the `AsRawSocket` trait.
///
/// **Note:** The methods only send data to the first resolved address when provided with multiple
/// addresses via the `ToSocketAddrs` trait. If no address is resolved, an error is returned.
///
/// # Example
///
/// ```rust
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
pub trait AsyncSendTo: AsRawSocket {
    /// Asynchronously sends data to the specified address. The method only sends to the first
    /// address resolved from the `ToSocketAddrs` input. Returns the number of bytes sent.
    ///
    /// # Example
    ///
    /// ```rust
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
        let buf_ptr = &ptr::from_ref(buf);

        SendTo::new(
            AsRawSocket::as_raw_socket(self),
            buf_ptr,
            &sock_addr_from_to_socket_addr(addr)?,
        )
        .await
    }

    /// Asynchronously sends data to the specified address with a deadline. Only the first resolved
    /// address is used. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Example
    ///
    /// ```rust
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
        let buf_ptr = &ptr::from_ref(buf);

        SendToWithDeadline::new(
            AsRawSocket::as_raw_socket(self),
            buf_ptr,
            &sock_addr_from_to_socket_addr(addr)?,
            deadline,
        )
        .await
    }

    /// Asynchronously sends data to the specified address with a timeout.
    /// Only the first resolved address is used. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Example
    ///
    /// ```rust
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
        let buf_ptr = &ptr::from_ref(buf);

        SendToWithDeadline::new(
            AsRawSocket::as_raw_socket(self),
            buf_ptr,
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
    /// ```rust
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
            let buf_ptr = &ptr::from_ref(&buf[sent..]);

            sent += SendTo::new(AsRawSocket::as_raw_socket(self), buf_ptr, &addr).await?;
        }

        Ok(sent)
    }

    /// Asynchronously sends all the data in the buffer to the specified address with a deadline.
    /// The method ensures that the entire buffer is transmitted before the deadline by repeatedly
    /// calling `send_to_with_deadline`. Only the first resolved address is used.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Example
    ///
    /// ```rust
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
            let buf_ptr = &ptr::from_ref(&buf[sent..]);

            sent +=
                SendToWithDeadline::new(AsRawSocket::as_raw_socket(self), buf_ptr, &addr, deadline)
                    .await?;
        }

        Ok(sent)
    }

    /// Asynchronously sends all the data in the buffer to the specified address with a timeout.
    /// The method ensures that the entire buffer is transmitted before the timeout by repeatedly
    /// calling `send_to_with_timeout`. Only the first resolved address is used.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Example
    ///
    /// ```rust
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

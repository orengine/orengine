use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{AsRawSocket, MessageSendHeader, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::FixedBuffer;
use crate::local_executor;
use crate::net::addr::{FromSockAddr, IntoSockAddr, ToSockAddrs};
use crate::net::Socket;
use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;
use std::future::Future;
use std::io;
use std::io::{ErrorKind, IoSlice, Result};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

/// `send_to` io operation.
#[repr(C)]
pub struct SendTo<'fut> {
    raw_socket: RawSocket,
    message_header: MessageSendHeader,
    bufs: &'fut [IoSlice<'fut>],
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>,
}

impl<'fut> SendTo<'fut> {
    /// Creates a new `send_to` io operation.
    pub fn new(raw_socket: RawSocket, bufs: &'fut [IoSlice<'fut>], addr: &'fut SockAddr) -> Self {
        Self {
            raw_socket,
            message_header: MessageSendHeader::new(),
            bufs,
            addr,
            io_request_data: None,
        }
    }
}

impl Future for SendTo<'_> {
    type Output = Result<usize>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        let os_message_header_ptr = this
            .message_header
            .get_os_message_header_ptr(this.addr, this.bufs);

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
#[repr(C)]
pub struct SendToWithDeadline<'fut> {
    raw_socket: RawSocket,
    message_header: MessageSendHeader,
    bufs: &'fut [IoSlice<'fut>],
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
    phantom_data: PhantomData<&'fut [u8]>,
}

impl<'fut> SendToWithDeadline<'fut> {
    /// Creates a new `send_to` io operation with deadline.
    pub fn new(
        raw_socket: RawSocket,
        bufs: &'fut [IoSlice<'fut>],
        addr: &'fut SockAddr,
        deadline: Instant,
    ) -> Self {
        Self {
            raw_socket,
            message_header: MessageSendHeader::new(),
            bufs,
            addr,
            io_request_data: None,
            deadline,
            phantom_data: PhantomData,
        }
    }
}

impl Future for SendToWithDeadline<'_> {
    type Output = Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        let worker = local_worker();
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        let os_message_header_ptr = this
            .message_header
            .get_os_message_header_ptr(this.addr, this.bufs);

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

#[inline]
/// Returns first resolved address from `ToSocketAddrs`.
fn sock_addr_from_to_socket_addr<Addr: IntoSockAddr + FromSockAddr, A: ToSockAddrs<Addr>>(
    to_addr: &A,
) -> Result<SockAddr> {
    let mut addrs = to_addr.to_sock_addrs()?;
    if let Some(addr) = addrs.next() {
        return Ok(addr.into_sock_addr());
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
/// or timeouts. The trait can be implemented for datagram-oriented [`sockets`](Socket).
///
/// **Note:** The methods only send data to the first resolved address when provided with multiple
/// addresses via the `ToSocketAddrs` trait. If no address is resolved, an error is returned.
///
/// # Example
///
/// ```rust
/// use orengine::net::UdpSocket;
/// use orengine::io::{buffer, AsyncBind, AsyncSendTo};
/// use std::net::SocketAddr;
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
/// let mut buf = buffer();
/// buf.append(b"Hello, World!");
///
/// // Send data to the specified address
/// socket.send_to(&buf, "127.0.0.1:8080").await?;
/// # Ok(())
/// # }
/// ```
///
/// # If coping `SocketAddr` is a performance issue
///
/// Then you can use structs [`SendTo`] and [`SendToWithDeadline`] that accepts only
/// a reference to [`SockAddr`] that can be created with [`ToSockAddrs`] one time and then reused.
pub trait AsyncSendTo: Socket {
    /// Asynchronously sends data to the specified address. The method only sends to the first
    /// address resolved from the [`ToSockAddrs`] input. Returns the number of bytes sent.
    ///
    /// # Difference between `send_bytes_to` and `send_to`
    ///
    /// Use [`send_to`](Self::send_to) if it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
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
    /// socket.send_bytes_to(b"Hello, World!", "127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_bytes_to<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &[u8],
        addr: A,
    ) -> Result<usize> {
        let bufs_ptr = &[IoSlice::new(buf)];

        SendTo::new(
            AsRawSocket::as_raw_socket(self),
            bufs_ptr,
            &sock_addr_from_to_socket_addr(&addr)?,
        )
        .await
    }

    /// Asynchronously sends data to the specified address. The method only sends to the first
    /// address resolved from the [`ToSockAddrs`] input. Returns the number of bytes sent.
    ///
    /// # Difference between `send_to` and `send_bytes_to`
    ///
    /// Use [`send_to`](Self::send_to) if it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{buffer, AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let mut buf = buffer();
    /// buf.append(b"Hello, World!");
    ///
    /// socket.send_to(&buf, "127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_to<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &impl FixedBuffer,
        addr: A,
    ) -> Result<usize> {
        // Now SendTo with `fixed` buffer is unsupported.
        self.send_bytes_to(buf.as_bytes(), addr).await
    }

    /// Asynchronously sends data to the specified address with a deadline. Only the first resolved
    /// address is used. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Difference between `send_bytes_to_with_deadline` and `send_to_with_deadline`
    ///
    /// Use [`send_to_with_deadline`](Self::send_to_with_deadline) if it is possible, because
    /// [`Buffer`](crate::io::Buffer) can be __fixed__.
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
    /// socket.send_bytes_to_with_deadline(data, addr, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_bytes_to_with_deadline<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &[u8],
        addr: A,
        deadline: Instant,
    ) -> Result<usize> {
        let bufs_ptr = &[IoSlice::new(buf)];

        SendToWithDeadline::new(
            AsRawSocket::as_raw_socket(self),
            bufs_ptr,
            &sock_addr_from_to_socket_addr(&addr)?,
            deadline,
        )
        .await
    }

    /// Asynchronously sends data to the specified address with a deadline. Only the first resolved
    /// address is used. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Difference between `send_to_with_deadline` and `send_bytes_to_with_deadline`
    ///
    /// Use [`send_to_with_deadline`](Self::send_to_with_deadline) if it is possible, because
    /// [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{buffer, AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    /// use std::time::{Duration, Instant};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let mut buf = buffer();
    /// buf.append(b"Hello, World!");
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// socket.send_to_with_deadline(&buf, addr, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_to_with_deadline<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &impl FixedBuffer,
        addr: A,
        deadline: Instant,
    ) -> Result<usize> {
        // Now SendTo with `fixed` buffer is unsupported.
        self.send_bytes_to_with_deadline(buf.as_bytes(), addr, deadline)
            .await
    }

    /// Asynchronously sends data to the specified address with a timeout.
    /// Only the first resolved address is used. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Difference between `send_bytes_to_with_timeout` and `send_to_with_timeout`
    ///
    /// Use [`send_to_with_timeout`](Self::send_to_with_timeout) if it is possible, because
    /// [`Buffer`](crate::io::Buffer) can be __fixed__.
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
    /// socket.send_bytes_to_with_timeout(data, addr, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_bytes_to_with_timeout<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &[u8],
        addr: A,
        timeout: Duration,
    ) -> Result<usize> {
        let bufs_ptr = &[IoSlice::new(buf)];

        SendToWithDeadline::new(
            AsRawSocket::as_raw_socket(self),
            bufs_ptr,
            &sock_addr_from_to_socket_addr(&addr)?,
            local_executor().start_round_time_for_deadlines() + timeout,
        )
        .await
    }

    /// Asynchronously sends data to the specified address with a timeout.
    /// Only the first resolved address is used. Returns the number of bytes sent.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Difference between `send_to_with_timeout` and `send_bytes_to_with_timeout`
    ///
    /// Use [`send_to_with_timeout`](Self::send_to_with_timeout) if it is possible, because
    /// [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{buffer, AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let mut buf = buffer();
    /// buf.append(b"Hello, World!");
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let timeout = Duration::from_secs(5);
    /// socket.send_to_with_timeout(&buf, addr, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_to_with_timeout<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &impl FixedBuffer,
        addr: A,
        timeout: Duration,
    ) -> Result<usize> {
        // Now SendTo with `fixed` buffer is unsupported.
        self.send_bytes_to_with_timeout(buf.as_bytes(), addr, timeout)
            .await
    }

    /// Asynchronously sends all the data in the buffer to the specified address. The method
    /// ensures that the entire buffer is transmitted by repeatedly calling `send_to` until all
    /// the data is sent. Only the first resolved address is used.
    ///
    /// # Difference between `send_all_bytes_to` and `send_all_to`
    ///
    /// Use [`send_all_to`](Self::send_all_to) if it is possible, because
    /// [`Buffer`](crate::io::Buffer) can be __fixed__.
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
    /// socket.send_all_bytes_to(data, addr).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_all_bytes_to<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &[u8],
        addr: A,
    ) -> Result<usize> {
        let mut sent = 0;
        let addr = sock_addr_from_to_socket_addr(&addr)?;

        while sent < buf.len() {
            let bufs_ptr = &[IoSlice::new(&buf[sent..])];

            sent += SendTo::new(AsRawSocket::as_raw_socket(self), bufs_ptr, &addr).await?;
        }

        Ok(sent)
    }

    /// Asynchronously sends all the data in the buffer to the specified address. The method
    /// ensures that the entire buffer is transmitted by repeatedly calling `send_to` until all
    /// the data is sent. Only the first resolved address is used.
    ///
    /// # Difference between `send_all_to` and `send_all_bytes_to`
    ///
    /// Use [`send_all_to`](Self::send_all_to) if it is possible, because
    /// [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{buffer, AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let mut buf = buffer();
    /// buf.append(b"Hello, World!");
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// socket.send_all_to(&buf, addr).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_all_to<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &impl FixedBuffer,
        addr: A,
    ) -> Result<usize> {
        // Now SendTo with `fixed` buffer is unsupported.
        self.send_all_bytes_to(buf.as_bytes(), addr).await
    }

    /// Asynchronously sends all the data in the buffer to the specified address with a deadline.
    /// The method ensures that the entire buffer is transmitted before the deadline by repeatedly
    /// calling `send_to_with_deadline`. Only the first resolved address is used.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Difference between `send_all_bytes_to_with_deadline` and `send_all_to_with_deadline`
    ///
    /// Use [`send_all_to_with_deadline`](Self::send_all_to_with_deadline) if
    /// it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
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
    /// socket.send_all_bytes_to_with_deadline(data, addr, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_all_bytes_to_with_deadline<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &[u8],
        addr: A,
        deadline: Instant,
    ) -> Result<usize> {
        let mut sent = 0;
        let addr = sock_addr_from_to_socket_addr(&addr)?;

        while sent < buf.len() {
            let bufs_ptr = &[IoSlice::new(&buf[sent..])];

            sent += SendToWithDeadline::new(
                AsRawSocket::as_raw_socket(self),
                bufs_ptr,
                &addr,
                deadline,
            )
            .await?;
        }

        Ok(sent)
    }

    /// Asynchronously sends all the data in the buffer to the specified address with a deadline.
    /// The method ensures that the entire buffer is transmitted before the deadline by repeatedly
    /// calling `send_to_with_deadline`. Only the first resolved address is used.
    ///
    /// # Difference between `send_all_to_with_deadline` and `send_all_bytes_to_with_deadline`
    ///
    /// Use [`send_all_to_with_deadline`](Self::send_all_to_with_deadline) if
    /// it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{buffer, AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    /// use std::time::{Duration, Instant};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let mut buf = buffer();
    /// buf.append(b"Hello, World!");
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// socket.send_all_to_with_deadline(&buf, addr, deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_all_to_with_deadline<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &impl FixedBuffer,
        addr: A,
        deadline: Instant,
    ) -> Result<usize> {
        // Now SendTo with `fixed` buffer is unsupported.
        self.send_all_bytes_to_with_deadline(buf.as_bytes(), addr, deadline)
            .await
    }

    /// Asynchronously sends all the data in the buffer to the specified address with a timeout.
    /// The method ensures that the entire buffer is transmitted before the timeout by repeatedly
    /// calling `send_to_with_timeout`. Only the first resolved address is used.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Difference between `send_all_bytes_to_with_timeout` and `send_all_to_with_timeout`
    ///
    /// Use [`send_all_to_with_timeout`](Self::send_all_to_with_timeout) if
    /// it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
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
    /// socket.send_all_bytes_to_with_timeout(data, addr, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_all_bytes_to_with_timeout<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &[u8],
        addr: A,
        timeout: Duration,
    ) -> Result<usize> {
        self.send_all_bytes_to_with_deadline(
            buf,
            addr,
            local_executor().start_round_time_for_deadlines() + timeout,
        )
        .await
    }

    /// Asynchronously sends all the data in the buffer to the specified address with a timeout.
    /// The method ensures that the entire buffer is transmitted before the timeout by repeatedly
    /// calling `send_to_with_timeout`. Only the first resolved address is used.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`].
    ///
    /// # Difference between `send_all_to_with_timeout` and `send_all_bytes_to_with_timeout`
    ///
    /// Use [`send_all_to_with_timeout`](Self::send_all_to_with_timeout) if
    /// it is possible, because [`Buffer`](crate::io::Buffer) can be __fixed__.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{buffer, AsyncBind, AsyncSendTo};
    /// use std::net::SocketAddr;
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut socket = UdpSocket::bind("0.0.0.0:0").await?;
    /// let mut buf = buffer();
    /// buf.append(b"Hello, World!");
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let timeout = Duration::from_secs(5);
    /// socket.send_all_to_with_timeout(&buf, addr, timeout).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn send_all_to_with_timeout<A: ToSockAddrs<Self::Addr>>(
        &mut self,
        buf: &impl FixedBuffer,
        addr: A,
        timeout: Duration,
    ) -> Result<usize> {
        // Now SendTo with `fixed` buffer is unsupported.
        self.send_all_bytes_to_with_timeout(buf.as_bytes(), addr, timeout)
            .await
    }
}

use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys;
use crate::io::sys::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use crate::local_executor;
use crate::net::addr::{IntoSockAddr, ToSockAddrs};
use crate::net::{ConnectedDatagram, Socket};
use crate::utils::each_addr::each_addr;

/// `connect` io operation.
#[repr(C)]
pub struct Connect<'fut> {
    raw_fd: RawSocket,
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>,
}

impl<'fut> Connect<'fut> {
    /// Creates a new `connect` io operation.
    pub fn new(raw_fd: RawSocket, addr: &'fut SockAddr) -> Self {
        Self {
            raw_fd,
            addr,
            io_request_data: None,
        }
    }
}

impl Future for Connect<'_> {
    type Output = Result<()>;

    #[allow(
        clippy::cast_ptr_alignment,
        reason = "sys::os_sockaddr is aligned rightly"
    )]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().connect(
                this.raw_fd,
                this.addr.as_ptr().cast::<sys::os_sockaddr>(),
                this.addr.len(),
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) }
            ),
            ()
        ));
    }
}

unsafe impl Send for Connect<'_> {}

/// `connect` io operation with deadline.
#[repr(C)]
pub struct ConnectWithDeadline<'fut> {
    raw_fd: RawSocket,
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
}

impl<'fut> ConnectWithDeadline<'fut> {
    /// Creates a new `connect` io operation with deadline.
    pub fn new(raw_fd: RawSocket, addr: &'fut SockAddr, deadline: Instant) -> Self {
        Self {
            raw_fd,
            addr,
            io_request_data: None,
            deadline,
        }
    }
}

impl Future for ConnectWithDeadline<'_> {
    type Output = Result<()>;

    #[allow(
        clippy::cast_ptr_alignment,
        reason = "sys::os_sockaddr is aligned rightly"
    )]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        let worker = local_worker();
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_time_bounded_io_request!((
            worker.connect_with_deadline(
                this.raw_fd,
                this.addr.as_ptr().cast::<sys::os_sockaddr>(),
                this.addr.len(),
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) },
                &mut this.deadline
            ),
            ()
        ));
    }
}

unsafe impl Send for ConnectWithDeadline<'_> {}

/// The `AsyncConnectStream` trait provides asynchronous methods for creating and connecting
/// stream-oriented sockets (like TCP) to a remote address.
///
/// This trait can be implemented for stream-oriented socket types that need
/// asynchronous connection functionality.
///
/// # Example
///
/// ```rust
/// use orengine::net::TcpStream;
/// use orengine::io::AsyncConnectStream;
///
/// # async fn foo() -> std::io::Result<()> {
/// // Connect to a remote address
/// let stream = TcpStream::connect("127.0.0.1:8080").await?;
///
/// // Or connect with a timeout
/// let stream = TcpStream::connect_with_timeout("127.0.0.1:8080", std::time::Duration::from_secs(10)).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncConnectStream: Sized + Socket {
    /// Creates a new socket based on the provided [`Addr`](Socket::Addr).
    ///
    /// # Warning
    ///
    /// Creates a socket, without connecting it, therefore you need to connect it.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::io;
    /// use std::io::ErrorKind;
    /// use std::net::SocketAddr;
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, Connect};
    /// use orengine::io::sys::AsRawSocket;
    /// use orengine::net::addr::IntoSockAddr;
    ///
    /// async fn foo() -> io::Result<()> {
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let stream = {
    ///     match TcpStream::new_for_addr(&addr).await {
    ///         Ok(not_connected_stream) => {
    ///             Connect::new(
    ///                 AsRawSocket::as_raw_socket(&not_connected_stream),
    ///                 &addr.into_sock_addr(),
    ///             )
    ///             .await
    ///             .map(|_| not_connected_stream)
    ///         }
    ///         Err(e) => Err(e)
    ///     }
    /// };
    /// # Ok(())
    /// # }
    /// ```
    async fn new_for_addr(addr: &Self::Addr) -> Result<Self>;

    /// Asynchronously connects a stream socket to the specified address.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::AsyncConnectStream;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn connect<A: ToSockAddrs<Self::Addr>>(addr: A) -> Result<Self> {
        each_addr(addr, move |addr| async move {
            let stream = Self::new_for_addr(&addr).await?;
            Connect::new(
                <Self as AsRawSocket>::as_raw_socket(&stream),
                &addr.into_sock_addr(),
            )
            .await?;

            Ok(stream)
        })
        .await
    }

    /// Asynchronously connects a stream socket to the specified address, enforcing a deadline.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::AsyncConnectStream;
    /// use std::time::{Instant, Duration};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// let stream = TcpStream::connect_with_deadline("127.0.0.1:8080", deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn connect_with_deadline<A: ToSockAddrs<Self::Addr>>(
        addr: A,
        deadline: Instant,
    ) -> Result<Self> {
        each_addr(addr, move |addr| async move {
            let stream = Self::new_for_addr(&addr).await?;
            ConnectWithDeadline::new(
                <Self as AsRawSocket>::as_raw_socket(&stream),
                &addr.into_sock_addr(),
                deadline,
            )
            .await?;

            Ok(stream)
        })
        .await
    }

    /// Asynchronously connects a stream socket to the specified address, enforcing a timeout.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::io::AsyncConnectStream;
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let stream = TcpStream::connect_with_timeout("127.0.0.1:8080", Duration::from_secs(10)).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn connect_with_timeout<A: ToSockAddrs<Self::Addr>>(
        addr: A,
        timeout: Duration,
    ) -> Result<Self> {
        Self::connect_with_deadline(
            addr,
            local_executor().start_round_time_for_deadlines() + timeout,
        )
        .await
    }
}

/// The `AsyncConnectDatagram` trait provides asynchronous methods for "connecting" a datagram socket
/// (like UDP) to a remote address.
///
/// Although datagram-oriented protocols are connectionless,
/// this implementation provides an interface to set an address where data should
/// be sent and received from. After setting a remote address with
/// [`connect`](AsyncConnectDatagram::connect), data can be sent to and received from that
///  address with [`send`](crate::io::AsyncSend) and [`recv`](crate::io::AsyncRecv).
///
/// It also supports connection timeouts and deadlines.
///
/// This trait can be implemented for datagram-oriented socket types.
///
/// # Example
///
/// ```rust
/// use orengine::net::UdpSocket;
/// use orengine::io::{AsyncBind, AsyncConnectDatagram};
///
/// # async fn foo() -> std::io::Result<()> {
/// let socket = UdpSocket::bind("127.0.0.1:8081").await?;
/// let connected_datagram = socket.connect("127.0.0.1:8080").await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncConnectDatagram<CD: ConnectedDatagram>: Socket + Sized {
    /// Asynchronously connects a datagram socket to the specified address.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{full_buffer, AsyncBind, AsyncConnectDatagram, AsyncPollSocket, AsyncRecv, AsyncSend};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:8081").await?;
    /// let mut connected_datagram = socket.connect("127.0.0.1:8080").await?;
    ///
    /// loop {
    ///     connected_datagram.poll_recv().await?;
    ///     let mut buf = full_buffer();
    ///     let n = connected_datagram.recv(&mut buf).await?;
    ///     if n == 0 {
    ///         break;
    ///     }
    ///
    ///     connected_datagram.send_all(&buf.slice(..n)).await?;
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn connect<A: ToSockAddrs<CD::Addr>>(self, addr: A) -> Result<CD> {
        let new_datagram_socket_raw_fd = IntoRawSocket::into_raw_socket(self);
        each_addr(addr, move |addr| async move {
            Connect::new(new_datagram_socket_raw_fd, &addr.into_sock_addr()).await?;
            Ok(unsafe { <CD as FromRawSocket>::from_raw_socket(new_datagram_socket_raw_fd) })
        })
        .await
    }

    /// Asynchronously connects a datagram socket to the specified address, enforcing a deadline.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{full_buffer, AsyncBind, AsyncConnectDatagram, AsyncPollSocket, AsyncRecv, AsyncSend};
    /// use std::time::{Instant, Duration};
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:8081").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// let mut connected_datagram = socket.connect_with_deadline("127.0.0.1:8080", deadline).await?;
    ///
    /// loop {
    ///     connected_datagram.poll_recv().await?;
    ///     let mut buf = full_buffer();
    ///     let n = connected_datagram.recv(&mut buf).await?;
    ///     if n == 0 {
    ///         break;
    ///     }
    ///
    ///     connected_datagram.send_all(&buf.slice(..n)).await?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn connect_with_deadline<A: ToSockAddrs<CD::Addr>>(
        self,
        addr: A,
        deadline: Instant,
    ) -> Result<CD> {
        let new_datagram_socket_raw_fd = IntoRawSocket::into_raw_socket(self);
        each_addr(addr, move |addr| async move {
            ConnectWithDeadline::new(new_datagram_socket_raw_fd, &addr.into_sock_addr(), deadline)
                .await?;
            Ok(unsafe { <CD as FromRawSocket>::from_raw_socket(new_datagram_socket_raw_fd) })
        })
        .await
    }

    /// Asynchronously connects a datagram socket to the specified address, enforcing a timeout.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{full_buffer, AsyncBind, AsyncConnectDatagram, AsyncPollSocket, AsyncRecv, AsyncSend};
    /// use std::time::Duration;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:8081").await?;
    /// let mut connected_datagram = socket.connect_with_timeout("127.0.0.1:8080", Duration::from_secs(10)).await?;
    ///
    /// loop {
    ///     connected_datagram.poll_recv().await?;
    ///     let mut buf = full_buffer();
    ///     let n = connected_datagram.recv(&mut buf).await?;
    ///     if n == 0 {
    ///         break;
    ///     }
    ///
    ///     connected_datagram.send_all(&buf.slice(..n)).await?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    async fn connect_with_timeout<A: ToSockAddrs<CD::Addr>>(
        self,
        addr: A,
        timeout: Duration,
    ) -> Result<CD> {
        self.connect_with_deadline(
            addr,
            local_executor().start_round_time_for_deadlines() + timeout,
        )
        .await
    }
}

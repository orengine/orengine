use std::future::Future;
use std::io::Result;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;
use crate::each_addr;

use crate::io::io_request_data::IoRequestData;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsRawFd, RawFd, IntoRawFd, FromRawFd};
use crate::io::worker::{local_worker, IoWorker};

/// `connect` io operation.
#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Connect<'fut> {
    fd: RawFd,
    addr: &'fut SockAddr,
    io_request_data: Option<IoRequestData>
}

impl<'fut> Connect<'fut> {
    /// Creates a new `connect` io operation.
    pub fn new(fd: RawFd, addr: &'fut SockAddr) -> Self {
        Self {
            fd,
            addr,
            io_request_data: None
        }
    }
}

impl<'fut> Future for Connect<'fut> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
            worker.connect(
                this.fd,
                this.addr.as_ptr(),
                this.addr.len(),
                this.io_request_data.as_mut().unwrap_unchecked()
            ),
            ()
        ));
    }
}

/// `connect` io operation with deadline.
#[must_use = "Future must be awaited to drive the IO operation"]
pub struct ConnectWithDeadline<'fut> {
    fd: RawFd,
    addr: &'fut SockAddr,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request_data: Option<IoRequestData>
}

impl<'fut> ConnectWithDeadline<'fut> {
    /// Creates a new `connect` io operation with deadline.
    pub fn new(fd: RawFd, addr: &'fut SockAddr, deadline: Instant) -> Self {
        Self {
            fd,
            addr,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request_data: None
        }
    }
}

impl<'fut> Future for ConnectWithDeadline<'fut> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_time_bounded_io_request!((
            worker.connect(
                this.fd,
                this.addr.as_ptr(),
                this.addr.len(),
                this.io_request_data.as_mut().unwrap_unchecked()
            ),
            ()
        ));
    }
}

/// The `AsyncConnectStream` trait provides asynchronous methods for creating and connecting
/// stream-oriented sockets (like TCP) to a remote address. It supports both IPv4 and IPv6, as well
/// as connection timeouts and deadlines.
///
/// This trait can be implemented for stream-oriented socket types that need
/// asynchronous connection functionality.
///
/// # Example
///
/// ```no_run
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
pub trait AsyncConnectStream: Sized + AsRawFd {
    /// Creates a new IPv4 socket for stream-based communication.
    ///
    /// # Warning
    ///
    /// Not connect returning stream, therefore you need to connect it.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::AsyncConnectStream;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let stream = TcpStream::new_ip4().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn new_ip4() -> Result<Self>;
    /// Creates a new IPv6 socket for stream-based communication.
    ///
    /// # Warning
    ///
    /// Not connect returning stream, therefore you need to connect it.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::AsyncConnectStream;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let stream = TcpStream::new_ip6().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn new_ip6() -> Result<Self>;

    /// Creates a new socket based on the provided [`SocketAddr`], automatically choosing between
    /// IPv4 and IPv6.
    ///
    /// # Warning
    ///
    /// Not connect returning stream, therefore you need to connect it.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::net::SocketAddr;
    /// use orengine::net::TcpStream;
    /// use orengine::io::AsyncConnectStream;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let stream = TcpStream::new_for_addr(&addr).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn new_for_addr(addr: &SocketAddr) -> Result<Self> {
        match addr {
            SocketAddr::V4(_) => Self::new_ip4().await,
            SocketAddr::V6(_) => Self::new_ip6().await,
        }
    }

    /// Asynchronously connects a stream socket to the specified address.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::AsyncConnectStream;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn connect<A: ToSocketAddrs>(addr: A) -> Result<Self> {
        each_addr!(
            &addr,
            async move |addr: SocketAddr| -> Result<Self> {
                let stream = Self::new_for_addr(&addr).await?;
                Connect::new(stream.as_raw_fd(), &SockAddr::from(addr)).await?;

                Ok(stream)
            }
        )
    }

    /// Asynchronously connects a stream socket to the specified address, enforcing a deadline.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
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
    #[inline(always)]
    async fn connect_with_deadline<A: ToSocketAddrs>(addr: A, deadline: Instant) -> Result<Self> {
        each_addr!(
            &addr,
            async move |addr: SocketAddr| -> Result<Self> {
                let stream = Self::new_for_addr(&addr).await?;
                ConnectWithDeadline::new(stream.as_raw_fd(), &SockAddr::from(addr), deadline).await?;

                Ok(stream)
            }
        )
    }

    /// Asynchronously connects a stream socket to the specified address, enforcing a timeout.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    /// use orengine::io::AsyncConnectStream;
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let stream = TcpStream::connect_with_timeout("127.0.0.1:8080", Duration::from_secs(10)).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn connect_with_timeout<A: ToSocketAddrs>(addr: A, timeout: Duration) -> Result<Self> {
        Self::connect_with_deadline(addr, Instant::now() + timeout).await
    }
}

/// The `AsyncConnectDatagram` trait provides asynchronous methods for connecting a datagram socket
/// (like UDP) to a remote address. It also supports connection timeouts and deadlines.
///
/// This trait can be implemented for datagram-oriented socket types.
///
/// # Example
///
/// ```no_run
/// use orengine::net::UdpSocket;
/// use orengine::io::{AsyncBind, AsyncConnectDatagram};
///
/// # async fn foo() -> std::io::Result<()> {
/// let socket = UdpSocket::bind("127.0.0.1:8081").await?;
/// let remote_socket = socket.connect("127.0.0.1:8080").await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncConnectDatagram<S: FromRawFd + Sized>: IntoRawFd + Sized {
    /// Asynchronously connects a datagram socket to the specified address.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncConnectDatagram};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:8081").await?;
    /// let remote_socket = socket.connect("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn connect<A: ToSocketAddrs>(self, addr: A) -> Result<S> {
        let new_datagram_socket_fd = self.into_raw_fd();
        each_addr!(
            &addr,
            async move |addr: SocketAddr| -> Result<S> {
                Connect::new(new_datagram_socket_fd, &SockAddr::from(addr)).await?;
                Ok(unsafe { S::from_raw_fd(new_datagram_socket_fd) })
            }
        )
    }

    /// Asynchronously connects a datagram socket to the specified address, enforcing a deadline.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncConnectDatagram};
    /// use std::time::{Instant, Duration};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:8081").await?;
    /// let deadline = Instant::now() + Duration::from_secs(5);
    /// let remote_socket = socket.connect_with_deadline("127.0.0.1:8080", deadline).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn connect_with_deadline<A: ToSocketAddrs>(self, addr: A, deadline: Instant) -> Result<S> {
        let new_datagram_socket_fd = self.into_raw_fd();
        each_addr!(
            &addr,
            async move |addr: SocketAddr| -> Result<S> {
                ConnectWithDeadline::new(new_datagram_socket_fd, &SockAddr::from(addr), deadline).await?;
                Ok(unsafe { S::from_raw_fd(new_datagram_socket_fd) })
            }
        )
    }

    /// Asynchronously connects a datagram socket to the specified address, enforcing a timeout.
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::net::UdpSocket;
    /// use orengine::io::{AsyncBind, AsyncConnectDatagram};
    /// use std::time::Duration;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:8081").await?;
    /// let remote_socket = socket.connect_with_timeout("127.0.0.1:8080", Duration::from_secs(10)).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn connect_with_timeout<A: ToSocketAddrs>(self, addr: A, timeout: Duration) -> Result<S> {
        self.connect_with_deadline(addr, Instant::now() + timeout).await
    }
}
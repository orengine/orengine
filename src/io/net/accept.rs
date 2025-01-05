use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys;
use crate::io::sys::{os_sockaddr, AsRawSocket, FromRawSocket, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use crate::BUG_MESSAGE;
use orengine_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;
use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::mem;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

/// The same as [`SockAddr`] but in this crate we can use private fields.
struct SockAddrRaw {
    storage: sys::sockaddr_storage,
    len: sys::socklen_t,
}

impl SockAddrRaw {
    /// Creates a new empty `SockAddrRaw`.
    fn empty() -> Self {
        Self {
            storage: unsafe { mem::zeroed() },
            #[allow(
                clippy::cast_possible_truncation,
                reason = "size of SockAddr is less than u32::MAX"
            )]
            len: size_of::<SockAddr>() as _,
        }
    }

    /// Converts `SockAddrRaw` to [`SockAddr`].
    #[inline(always)]
    fn as_sock_addr(&self) -> SockAddr {
        unsafe { SockAddr::new(self.storage, self.len) }
    }
}

/// `accept` io operation.
pub struct Accept<S: FromRawSocket> {
    raw_socket: RawSocket,
    addr: SockAddrRaw,
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<S>,
}

impl<S: FromRawSocket> Accept<S> {
    /// Creates a new `accept` io operation.
    pub fn new(raw_socket: RawSocket) -> Self {
        Self {
            raw_socket,
            #[allow(
                clippy::cast_possible_truncation,
                reason = "size of SockAddr is less than u32::MAX"
            )]
            addr: SockAddrRaw::empty(),
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl<S: FromRawSocket> Future for Accept<S> {
    type Output = Result<(S, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().accept(
                this.raw_socket,
                (&raw mut this.addr.storage).cast::<os_sockaddr>(),
                &raw mut this.addr.len,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) },
            ),
            unsafe {
                (
                    S::from_raw_socket(ret as RawSocket),
                    this.addr.as_sock_addr(),
                )
            }
        ));
    }
}

unsafe impl<S: FromRawSocket> Send for Accept<S> {}

/// `accept` io operation with deadline.
pub struct AcceptWithDeadline<S: FromRawSocket> {
    raw_socket: RawSocket,
    addr: SockAddrRaw,
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
    pin: PhantomData<S>,
}

impl<S: FromRawSocket> AcceptWithDeadline<S> {
    /// Creates a new `accept` io operation with deadline.
    pub fn new(raw_socket: RawSocket, deadline: Instant) -> Self {
        Self {
            raw_socket,
            #[allow(
                clippy::cast_possible_truncation,
                reason = "size of SockAddr is less than u32::MAX"
            )]
            addr: SockAddrRaw::empty(),
            io_request_data: None,
            deadline,
            pin: PhantomData,
        }
    }
}

impl<S: FromRawSocket> Future for AcceptWithDeadline<S> {
    type Output = Result<(S, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.accept_with_deadline(
                this.raw_socket,
                (&raw mut this.addr.storage).cast::<os_sockaddr>(),
                &raw mut this.addr.len,
                unsafe { IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked()) },
                &mut this.deadline
            ),
            unsafe {
                (
                    S::from_raw_socket(ret as RawSocket),
                    this.addr.as_sock_addr(),
                )
            }
        ));
    }
}

unsafe impl<S: FromRawSocket> Send for AcceptWithDeadline<S> {}

/// The `AsyncAccept` trait provides asynchronous methods for accepting new incoming connections.
///
/// It is implemented for types that can be represented as raw file descriptors (via [`AsRawSocket`]).
///
/// This trait allows the server-side of a socket to accept new connections either indefinitely or
/// with a specified timeout or deadline.
///
/// The accepted connection is returned as a stream of type `S`
/// (e.g., [`TcpStream`](crate::net::TcpStream)), along with the
/// remote socket address ([`SocketAddr`]).
///
/// # Example
///
/// ```rust
/// use orengine::net::TcpStream;
/// use orengine::net::TcpListener;
/// use orengine::io::{AsyncAccept, AsyncBind};
/// use std::net::SocketAddr;
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
///
/// // Accept a new connection
/// let (stream, addr): (TcpStream, SocketAddr) = listener.accept().await?;
///
/// // Use the stream for further communication with the client.
/// # Ok(())
/// # }
/// ```
pub trait AsyncAccept<S: FromRawSocket>: AsRawSocket {
    /// Asynchronously accepts a new incoming connection.
    ///
    /// This method listens for and accepts a new connection from a remote client. It returns the
    /// stream (`S`) and the remote socket address ([`SocketAddr`]) once a connection is successfully
    /// established.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::net::TcpListener;
    /// use orengine::io::{AsyncAccept, AsyncBind};
    /// use std::net::SocketAddr;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    /// // Accept a connection
    /// let (stream, addr): (TcpStream, SocketAddr) = listener.accept().await?;
    ///
    /// // Stream and address are now available
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn accept(&mut self) -> Result<(S, SocketAddr)> {
        let (stream, sock_addr) = Accept::<S>::new(sys::AsRawSocket::as_raw_socket(self)).await?;
        Ok((stream, sock_addr.as_socket().expect(BUG_MESSAGE)))
    }

    /// Asynchronously accepts a new connection, with a specified deadline.
    ///
    /// This method works similarly to [`accept`](Self::accept),
    /// but it will time out if the connection is not accepted by
    /// the specified `deadline` (using [`Instant`]).
    ///
    /// If the deadline is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::net::TcpListener;
    /// use orengine::io::{AsyncAccept, AsyncBind};
    /// use std::time::Instant;
    /// use std::net::SocketAddr;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// let deadline = Instant::now() + std::time::Duration::from_secs(5);
    ///
    /// // Accept a connection with a deadline
    /// let (stream, addr): (TcpStream, SocketAddr) = listener.accept_with_deadline(deadline).await?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn accept_with_deadline(&mut self, deadline: Instant) -> Result<(S, SocketAddr)> {
        let (stream, sock_addr) =
            AcceptWithDeadline::<S>::new(sys::AsRawSocket::as_raw_socket(self), deadline).await?;
        Ok((stream, sock_addr.as_socket().expect(BUG_MESSAGE)))
    }

    /// Asynchronously accepts a new connection, with a specified timeout.
    ///
    /// This method sets a timeout duration for accepting a new connection.
    /// If the timeout is exceeded, the method will return an error with
    /// kind [`ErrorKind::TimedOut`](std::io::ErrorKind::TimedOut).
    ///
    /// This method internally uses [`accept_with_deadline`](Self::accept_with_deadline)
    /// and calculates the deadline as `Instant::now() + timeout`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpStream;
    /// use orengine::net::TcpListener;
    /// use orengine::io::{AsyncAccept, AsyncBind};
    /// use std::time::Duration;
    /// use std::net::SocketAddr;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    /// // Accept a connection with a timeout
    /// let (stream, addr): (TcpStream, SocketAddr) = listener.accept_with_timeout(Duration::from_secs(5)).await?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn accept_with_timeout(&mut self, timeout: Duration) -> Result<(S, SocketAddr)> {
        self.accept_with_deadline(Instant::now() + timeout).await
    }
}

// TODO unix
// pub(crate) trait AsyncAcceptUnix<S: FromRawSocket>: AsRawSocket {
//     #[inline(always)]
//     async fn accept(&mut self) -> Result<(S, std::os::unix::net::SocketAddr)> {
//         let (stream, addr) = Accept::<S>::new(sys::AsRawSocket::as_raw_socket(self)).await?;
//         Ok((stream, addr.as_unix().expect(BUG)))
//     }
//
//     #[inline(always)]
//     async fn accept_with_deadline(
//         &mut self,
//         deadline: Instant
//     ) -> Result<(S, std::os::unix::net::SocketAddr)> {
//         let (stream, addr) = AcceptWithDeadline::<S>::new(sys::AsRawSocket::as_raw_socket(self), deadline).await?;
//         Ok((stream, addr.as_unix().expect(BUG)))
//     }
//
//     #[inline(always)]
//     async fn accept_with_timeout(
//         &mut self,
//         timeout: Duration
//     ) -> Result<(S, std::os::unix::net::SocketAddr)> {
//         self.accept_with_deadline(Instant::now() + timeout).await
//     }
// }

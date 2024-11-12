use nix::libc;
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

use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{AsRawFd, FromRawFd, RawFd};
use crate::io::worker::{local_worker, IoWorker};
use crate::BUG_MESSAGE;

/// `accept` io operation.
pub struct Accept<S: FromRawFd> {
    fd: RawFd,
    addr: (SockAddr, libc::socklen_t),
    io_request_data: Option<IoRequestData>,
    phantom_data: PhantomData<S>,
}

impl<S: FromRawFd> Accept<S> {
    /// Creates a new `accept` io operation.
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            #[allow(clippy::cast_possible_truncation)] // size of SockAddr is less than u32::MAX
            addr: (unsafe { mem::zeroed() }, size_of::<SockAddr>() as _),
            io_request_data: None,
            phantom_data: PhantomData,
        }
    }
}

impl<S: FromRawFd> Future for Accept<S> {
    type Output = Result<(S, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ret;

        poll_for_io_request!((
            local_worker().accept(
                this.fd,
                this.addr.0.as_ptr().cast_mut(),
                &mut this.addr.1,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() }
            ),
            unsafe { (S::from_raw_fd(ret as RawFd), this.addr.0.clone()) }
        ));
    }
}

/// `accept` io operation with deadline.
pub struct AcceptWithDeadline<S: FromRawFd> {
    fd: RawFd,
    addr: (SockAddr, libc::socklen_t),
    io_request_data: Option<IoRequestData>,
    deadline: Instant,
    pin: PhantomData<S>,
}

impl<S: FromRawFd> AcceptWithDeadline<S> {
    /// Creates a new `accept` io operation with deadline.
    pub fn new(fd: RawFd, deadline: Instant) -> Self {
        Self {
            fd,
            #[allow(clippy::cast_possible_truncation)] // size of SockAddr is less than u32::MAX
            addr: (unsafe { mem::zeroed() }, size_of::<SockAddr>() as _),
            io_request_data: None,
            deadline,
            pin: PhantomData,
        }
    }
}

impl<S: FromRawFd> Future for AcceptWithDeadline<S> {
    type Output = Result<(S, SockAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = local_worker();
        let ret;

        poll_for_time_bounded_io_request!((
            worker.accept_with_deadline(
                this.fd,
                this.addr.0.as_ptr().cast_mut(),
                &mut this.addr.1,
                unsafe { this.io_request_data.as_mut().unwrap_unchecked() },
                &mut this.deadline
            ),
            unsafe { (S::from_raw_fd(ret as RawFd), this.addr.0.clone()) }
        ));
    }
}

/// The `AsyncAccept` trait provides asynchronous methods for accepting new incoming connections.
///
/// It is implemented for types that can be represented as raw file descriptors (via [`AsRawFd`]).
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
pub trait AsyncAccept<S: FromRawFd>: AsRawFd {
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
        let (stream, sock_addr) = Accept::<S>::new(self.as_raw_fd()).await?;
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
        let (stream, sock_addr) = AcceptWithDeadline::<S>::new(self.as_raw_fd(), deadline).await?;
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
// pub(crate) trait AsyncAcceptUnix<S: FromRawFd>: AsRawFd {
//     #[inline(always)]
//     async fn accept(&mut self) -> Result<(S, std::os::unix::net::SocketAddr)> {
//         let (stream, addr) = Accept::<S>::new(self.as_raw_fd()).await?;
//         Ok((stream, addr.as_unix().expect(BUG)))
//     }
//
//     #[inline(always)]
//     async fn accept_with_deadline(
//         &mut self,
//         deadline: Instant
//     ) -> Result<(S, std::os::unix::net::SocketAddr)> {
//         let (stream, addr) = AcceptWithDeadline::<S>::new(self.as_raw_fd(), deadline).await?;
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

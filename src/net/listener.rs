use crate::io::sys::{AsRawSocket, AsSocket, FromRawSocket, IntoRawSocket};
use crate::io::{AsyncAccept, AsyncBind, AsyncSocketClose};
use crate::net::Stream;
use std::io;
use std::io::Error;
use std::net::SocketAddr;

/// The `Listener` trait defines common socket-related operations for types that implement
/// listening functionality, such as [`TCP socket listeners`](crate::net::TcpListener).
///
/// It is intended to be implemented for types that accept incoming connections
/// via the [`AsyncAccept`] trait and provides methods for querying and configuring socket settings,
/// such as TTL and obtaining the local address. Additionally, it offers the ability
/// to retrieve pending socket errors.
///
/// This trait also requires implementors to handle raw file descriptors, as it is
/// derived from multiple raw file descriptor traits such as `AsSocket`, `AsRawSocket`,
/// `FromRawSocket`, and `IntoRawSocket`.
///
/// # Implemented Traits
///
/// - [`AsyncAccept`]
/// - [`AsyncSocketClose`]
/// - [`AsyncBind`]
/// - [`FromRawSocket`]
/// - [`IntoRawSocket`]
/// - [`AsSocket`]
/// - [`AsRawSocket`]
pub trait Listener:
    AsyncAccept<Self::Stream>
    + FromRawSocket
    + AsyncSocketClose
    + AsyncBind
    + AsRawSocket
    + AsSocket
    + IntoRawSocket
{
    type Stream: Stream;

    /// Returns the local socket address that the listener is bound to.
    ///
    /// This method provides the local address, such as the IP and port, that the listener is
    /// currently bound to. This is useful for confirming the listener's binding information.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::AsyncBind;
    /// use orengine::net::{TcpListener, Listener};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// let local_addr = listener.local_addr()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn local_addr(&self) -> io::Result<SocketAddr> {
        let borrow_socket = AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref
            .local_addr()?
            .as_socket()
            .ok_or_else(|| Error::new(io::ErrorKind::Other, "failed to get local address"))
    }

    /// Sets the TTL (Time-To-Live) value for outgoing packets on the listener socket.
    ///
    /// The TTL value determines how many hops (routers) a packet can pass through before being discarded.
    /// Setting this value can control the network reach of packets sent by the listener socket.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::AsyncBind;
    /// use orengine::net::{TcpListener, Listener};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// listener.set_ttl(64)?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        let borrow_socket = AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.set_ttl(ttl)
    }

    /// Returns the current TTL (Time-To-Live) value for outgoing packets on the listener socket.
    ///
    /// The TTL value determines how many hops (routers) a packet can pass through before
    /// being discarded. This method retrieves the TTL value that is currently
    /// set on the listener socket.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::AsyncBind;
    /// use orengine::net::{TcpListener, Listener};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// let ttl = listener.ttl()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn ttl(&self) -> io::Result<u32> {
        let borrow_socket = AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.ttl()
    }

    /// Retrieves and clears any pending socket errors on the listener.
    ///
    /// This method allows you to check for any socket errors that occurred during socket operations,
    /// such as accepting connections or communication. Once retrieved, the error state is cleared.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::AsyncBind;
    /// use orengine::net::{TcpListener, Listener};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// if let Some(err) = listener.take_error()? {
    ///     println!("Socket error occurred: {}", err);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn take_error(&self) -> io::Result<Option<Error>> {
        let borrow_socket = AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.take_error()
    }
}

use crate::io::sys::{AsRawSocket, AsSocket, FromRawSocket, IntoRawSocket};
use crate::io::{AsyncPollSocket, AsyncSocketClose};
use crate::net::addr::{FromSockAddr, IntoSockAddr, ToSockAddrs};
use crate::net::unix::unsupport::new_unix_unsupported_error;
use std::io;
use std::io::Error;

/// The `Socket` trait defines common socket-related operations and is intended to be implemented
/// for types that represent network sockets.
///
/// It provides methods for querying and configuring
/// socket settings, such as TTL and obtaining the local address
/// or pending socket errors.
///
/// # Implemented traits
///
/// - [`AsyncPollSocket`]
/// - [`AsyncSocketClose`]
/// - [`IntoRawSocket`]
/// - [`AsRawSocket`]
/// - [`FromRawSocket`]
/// - [`AsSocket`]
/// - [`AsyncPollSocket`]
pub trait Socket:
    IntoRawSocket + AsRawSocket + FromRawSocket + AsSocket + AsyncPollSocket + AsyncSocketClose
{
    /// The address type associated with the socket. It is expected to be represented as a
    /// `std::net::SocketAddr` or `std::os::unix::net::SocketAddr` but can be any type that
    /// implements the [`IntoSockAddr`], [`FromSockAddr`]
    /// and [`ToSockAddrs<Self::Addr>`](ToSockAddrs) traits.
    type Addr: IntoSockAddr + FromSockAddr + ToSockAddrs<Self::Addr>;

    /// Returns whether the `Socket` is a unix socket.
    #[inline(always)]
    fn is_unix(&self) -> bool {
        false
    }

    /// Returns the local socket address that the listener is bound to.
    ///
    /// This method provides the local address, such as the IP and port, that the listener is
    /// currently bound to. This is useful for confirming the listener's binding information.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::AsyncBind;
    /// use orengine::net::{TcpListener, Listener, Socket};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// let local_addr = listener.local_addr()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn local_addr(&self) -> io::Result<Self::Addr> {
        let borrow_socket = AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);

        Self::Addr::from_sock_addr(socket_ref.local_addr()?)
            .ok_or_else(|| Error::new(io::ErrorKind::Other, "failed to get local address"))
    }

    /// Sets the TTL (Time-To-Live) value for outgoing packets on the listener socket.
    ///
    /// The TTL value determines how many hops (routers) a packet can pass through before being discarded.
    /// Setting this value can control the network reach of packets sent by the listener socket.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support TTL, therefore this method is empty for those sockets.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::AsyncBind;
    /// use orengine::net::{TcpListener, Listener, Socket};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// listener.set_ttl(64)?;
    ///
    /// assert_eq!(listener.ttl()?, 64);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

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
    /// # Unix
    ///
    /// UNIX sockets do not support TTL, therefore this method always returns `Ok(0)`
    /// for those sockets.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::AsyncBind;
    /// use orengine::net::{TcpListener, Listener, Socket};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// let ttl = listener.ttl()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn ttl(&self) -> io::Result<u32> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

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
    /// use orengine::net::{TcpListener, Listener, Socket};
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

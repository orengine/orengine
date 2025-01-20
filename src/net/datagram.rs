use crate::io::{sys, AsyncBind, AsyncConnectDatagram, AsyncPeekFrom, AsyncRecvFrom, AsyncSendTo};
use crate::net::connected_datagram::ConnectedDatagram;
use crate::net::new_unix_unsupported_error;
use crate::net::Socket;
use std::net::{Ipv4Addr, Ipv6Addr};

/// The `Datagram` trait defines common operations for connectionless datagram-based sockets,
/// such as UDP.
///
/// It extends the [`Socket`] trait and provides methods for sending and receiving
/// datagrams, as well as configuring settings like broadcast and multicast.
///
/// # Associated Types
///
/// - [`ConnectedDatagram`]: Represents a datagram that is connected to a specific remote address.
///
/// # Implemented Traits
///
/// - [`Socket`]
/// - [`AsyncPollSocket`](crate::io::AsyncPollSocket)
/// - [`AsyncClose`](crate::io::AsyncSocketClose)
/// - [`IntoRawSocket`](sys::IntoRawSocket)
/// - [`FromRawSocket`](sys::FromRawSocket)
/// - [`AsSocket`](sys::AsSocket)
/// - [`AsRawSocket`](sys::AsRawSocket)
/// - [`AsyncConnectDatagram`]
/// - [`AsyncRecvFrom`]
/// - [`AsyncPeekFrom`]
/// - [`AsyncSendTo`]
/// - [`AsyncBind`]
///
/// # Example
///
/// ```rust
/// use orengine::io::full_buffer;
/// use orengine::net::Datagram;
///
/// async fn handle_datagram<D: Datagram>(mut datagram: D) {
///     loop {
///        datagram.poll_recv().await.expect("poll failed");
///        let mut buf = full_buffer();
///        let (n, addr) = datagram.recv_bytes_from(&mut buf).await.expect("recv_from failed");
///        if n == 0 {
///            continue;
///        }
///
///        datagram.send_bytes_to(&buf[..n], addr).await.expect("send_to failed");
///     }
/// }
/// ```
pub trait Datagram:
    Socket
    + AsyncConnectDatagram<Self::ConnectedDatagram>
    + AsyncRecvFrom
    + AsyncPeekFrom
    + AsyncSendTo
    + AsyncBind
{
    /// Type of the connected datagram, which allows sending data without specifying the address
    /// for each operation.
    type ConnectedDatagram: ConnectedDatagram<Addr = Self::Addr>;

    /// Enables or disables broadcasting on the socket. When broadcasting is enabled (`true`),
    /// the socket can send packets to the broadcast address.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support broadcasting and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn set_broadcast(&self, broadcast: bool) -> std::io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.set_broadcast(broadcast)
    }

    /// Returns whether the broadcast option is enabled for the socket.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support broadcasting and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn broadcast(&self) -> std::io::Result<bool> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.broadcast()
    }

    /// Enables or disables IPv4 multicast loopback for the socket. When loopback is enabled
    /// (`true`), the socket will receive multicast packets that it sends.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn set_multicast_loop_v4(&self, multicast_loop_v4: bool) -> std::io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.set_multicast_loop_v4(multicast_loop_v4)
    }

    /// Returns whether IPv4 multicast loopback is enabled.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn multicast_loop_v4(&self) -> std::io::Result<bool> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.multicast_loop_v4()
    }

    /// Sets the TTL (time-to-live) value for IPv4 multicast packets.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn set_multicast_ttl_v4(&self, multicast_ttl_v4: u32) -> std::io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.set_multicast_ttl_v4(multicast_ttl_v4)
    }

    /// Returns the current TTL value for IPv4 multicast packets.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn multicast_ttl_v4(&self) -> std::io::Result<u32> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.multicast_ttl_v4()
    }

    /// Enables or disables IPv6 multicast loopback for the socket. When loopback is enabled
    /// (`true`), the socket will receive multicast packets that it sends.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn set_multicast_loop_v6(&self, multicast_loop_v6: bool) -> std::io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.set_multicast_loop_v6(multicast_loop_v6)
    }

    /// Returns whether IPv6 multicast loopback is enabled.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn multicast_loop_v6(&self) -> std::io::Result<bool> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.multicast_loop_v6()
    }

    /// Joins the socket to an IPv4 multicast group, specified by `multiaddr` and the `interface`.
    /// The `interface` is the address of the local network interface.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::{UdpSocket, Datagram};
    /// use std::net::Ipv4Addr;
    /// use orengine::io::AsyncBind;
    ///
    /// async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:8080").await?;
    /// socket.join_multicast_v4(&Ipv4Addr::new(224, 0, 0, 1), &Ipv4Addr::new(0, 0, 0, 0))?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    fn join_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr) -> std::io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.join_multicast_v4(multiaddr, interface)
    }

    /// Joins the socket to an IPv6 multicast group, specified by `multiaddr` and the `interface`.
    /// The `interface` is the index of the network interface.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always returns
    /// Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> std::io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.join_multicast_v6(multiaddr, interface)
    }

    /// Leaves the IPv4 multicast group that the socket had joined.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn leave_multicast_v4(
        &self,
        multiaddr: &Ipv4Addr,
        interface: &Ipv4Addr,
    ) -> std::io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.leave_multicast_v4(multiaddr, interface)
    }

    /// Leaves the IPv6 multicast group that the socket had joined.
    ///
    /// # Unix
    ///
    /// UNIX sockets do not support multicast and always
    /// returns Err([`ErrorKind::Unsupported`](std::io::ErrorKind::Unsupported)).
    #[inline]
    fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> std::io::Result<()> {
        if self.is_unix() {
            return Err(new_unix_unsupported_error());
        }

        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        socket_ref.leave_multicast_v6(multiaddr, interface)
    }
}

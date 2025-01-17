use crate::io::{sys, AsyncPeek, AsyncRecv, AsyncSend};
use crate::net::addr::FromSockAddr;
use crate::net::Socket;
use std::io;
use std::io::Error;

/// The `ConnectedDatagram` trait represents a datagram socket that has been connected to a specific
/// remote address.
///
/// Unlike regular datagram sockets, a connected datagram socket can send and receive
/// packets without specifying the destination address on each send.
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
/// - [`AsyncRecv`]
/// - [`AsyncPeek`]
/// - [`AsyncSend`]
///
/// # Example
///
/// ```rust
/// use orengine::io::full_buffer;
/// use orengine::net::ConnectedDatagram;
///
/// async fn handle_connected_datagram<CD: ConnectedDatagram>(mut connected_datagram: CD) {
///     loop {
///         connected_datagram.poll_recv().await.expect("poll failed");
///         let mut buf = full_buffer();
///         let n = connected_datagram.recv(&mut buf).await.expect("recv_from failed");
///         if n == 0 {
///             break;
///         }
///
///         connected_datagram.send(&buf.slice(..n)).await.expect("send_to failed");
///     }
/// }
/// ```
pub trait ConnectedDatagram: Socket + AsyncRecv + AsyncPeek + AsyncSend {
    /// Returns the socket address of the remote peer to which this datagram socket is connected.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::io::{AsyncBind, AsyncConnectDatagram};
    /// use orengine::net::{UdpSocket, ConnectedDatagram};
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let datagram = UdpSocket::bind("127.0.0.1:8080").await?;
    /// let connected_datagram = datagram.connect("127.0.0.1:8081").await?;
    /// let peer = connected_datagram.peer_addr()?;
    /// println!("Connected to: {}", peer);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peer_addr(&self) -> io::Result<Self::Addr> {
        let borrow_socket = sys::AsSocket::as_socket(self);
        let socket_ref = socket2::SockRef::from(&borrow_socket);
        Self::Addr::from_sock_addr(socket_ref.peer_addr()?)
            .ok_or_else(|| Error::new(io::ErrorKind::Other, "failed to get local address"))
    }
}

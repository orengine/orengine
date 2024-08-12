use std::io;
use std::io::Error;
use std::os::unix::net::SocketAddr;
use crate::io::{AsyncConnectStreamUnix, AsyncPeek, AsyncRecv, AsyncSend};

use crate::net::unix::path_socket::PathSocket;
use crate::net::UnixStream;

pub trait PathStream:
    PathSocket + AsyncSend + AsyncRecv + AsyncPeek + AsyncConnectStreamUnix {
    async fn pair() -> io::Result<(UnixStream, UnixStream)>;

    /// Returns the socket address of the remote peer of this TCP connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use orengine::io::AsyncConnectStream;
    /// use orengine::net::{Stream, TcpStream};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// assert_eq!(
    ///     stream.peer_addr().unwrap(),
    ///     SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.peer_addr()?.as_unix().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get peer address",
        ))
    }
}
use std::io;
use std::io::Error;
use std::net::SocketAddr;
use std::time::Duration;
use crate::io::{AsyncConnectStream, AsyncPeek, AsyncRecv, AsyncSend, AsyncShutdown};
use crate::net::Socket;

/// The `Stream` trait defines common operations for bidirectional communication streams, such as
/// TCP connections or similar. It extends the `Socket` trait and integrates asynchronous methods
/// for sending, receiving, and peeking data, as well as shutting down the stream. Additionally,
/// it provides methods for controlling socket options like linger and TCP_NODELAY, and querying
/// the peer address.
///
/// # Implemented Traits
///
/// - [`Socket`]
/// - [`AsyncPollFd`](crate::io::AsyncPollFd)
/// - [`AsyncClose`](crate::io::AsyncClose)
/// - [`IntoRawFd`](crate::io::sys::IntoRawFd)
/// - [`FromRawFd`](crate::io::sys::FromRawFd)
/// - [`AsFd`](crate::io::sys::AsFd)
/// - [`AsRawFd`](crate::io::sys::AsRawFd)
/// - [`AsyncConnectStream`]
/// - [`AsyncRecv`]
/// - [`AsyncPeek`]
/// - [`AsyncSend`]
/// - [`AsyncShutdown`]
///
/// # Example
///
/// ```no_run
/// use orengine::buf::full_buffer;
/// use orengine::local_executor;
/// use orengine::net::Stream;
///
/// async fn handle_stream<S: Stream>(mut stream: S) {
///     loop {
///         stream.poll_recv().await.expect("poll_recv was failed");
///         let mut buf = full_buffer();
///         let n = stream.recv(&mut buf).await.expect("recv was failed");
///         if n == 0 {
///             break;
///         }
///
///         stream.send_all(b"pong").await.expect("send_all was failed");
///     }
/// }
/// ```
pub trait Stream:
    Socket + AsyncConnectStream + AsyncRecv + AsyncPeek + AsyncSend + AsyncShutdown {
    /// Sets the socket linger option, which controls the behavior when the stream is closed.
    /// If `Some(duration)` is provided, the system will try to send any unsent data before
    /// closing the connection for up to the specified duration. If `None` is provided, the
    /// system will immediately close the connection without attempting to send pending data.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::io::AsyncConnectStream;
    /// use orengine::net::{TcpStream, Stream};
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// stream.set_linger(Some(std::time::Duration::from_secs(5)))?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn set_linger(&self, linger: Option<Duration>) -> io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_linger(linger)
    }

    /// Returns the current socket linger option. This option indicates whether the stream
    /// is configured to attempt to send unsent data when it is closed and, if so, the
    /// duration for which it will attempt to do so.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::io::AsyncConnectStream;
    /// use orengine::net::{TcpStream, Stream};
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let linger = stream.linger()?;
    /// println!("Linger setting: {:?}", linger);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn linger(&self) -> io::Result<Option<Duration>> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.linger()
    }

    /// Sets the TCP_NODELAY option for the stream. When enabled (`true`), this option disables
    /// Nagle's algorithm, which reduces latency by sending small packets immediately. If
    /// disabled (`false`), small packets may be combined into larger ones for efficiency.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::io::AsyncConnectStream;
    /// use orengine::net::{TcpStream, Stream};
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// stream.set_nodelay(true)?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_nodelay(nodelay)
    }

    /// Returns the current state of the TCP_NODELAY option for the stream.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::io::AsyncConnectStream;
    /// use orengine::net::{TcpStream, Stream};
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let nodelay = stream.nodelay()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn nodelay(&self) -> io::Result<bool> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.nodelay()
    }

    /// Returns the socket address of the remote peer this stream is connected to.
    ///
    /// This method provides the remote address of the peer connected to this
    /// stream. This is useful for logging, debugging, or confirming connection details.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::io::AsyncConnectStream;
    /// use orengine::net::{TcpStream, Stream};
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let peer_addr = stream.peer_addr()?;
    /// println!("Connected to: {}", peer_addr);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.peer_addr()?.as_socket().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }
}
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::time::{Duration, Instant};
use std::{io, mem};

use socket2::{Domain, Socket, Type};

use crate::io::sys::{AsFd, Fd};
use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown, Connect, ConnectWithTimeout};
use crate::net::creators_of_sockets::new_unix_socket;
use crate::runtime::local_executor;
use crate::{
    each_addr, generate_connect_from_new_socket, generate_peek, generate_peek_exact, generate_recv,
    generate_recv_exact, generate_send, generate_send_all,
};

// TODO update docs
pub struct Stream {
    fd: Fd,
}

impl Stream {
    /// Returns the state_ptr of the [`Stream`].
    ///
    /// Uses for low-level work with the scheduler. If you don't know what it is, don't use it.
    #[inline(always)]
    pub fn fd(&mut self) -> Fd {
        self.fd
    }

    #[inline(always)]
    #[cfg(unix)]
    pub fn borrow_fd(&self) -> BorrowedFd {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }

    generate_connect_from_new_socket!(|_| -> io::Result<Socket> { new_unix_socket() });

    #[inline(always)]
    pub fn pair() -> io::Result<(Stream, Stream)> {
        let (socket1, socket2) = Socket::pair(Domain::UNIX, Type::STREAM, None)?;
        Ok((
            Stream {
                fd: socket1.into_raw_fd(),
            },
            Stream {
                fd: socket2.into_raw_fd(),
            },
        ))
    }

    generate_send!();

    generate_send_all!();

    generate_recv!();

    generate_recv_exact!();

    generate_peek!();

    generate_peek_exact!();

    /// Returns the socket address of the remote peer of this TCP connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use orengine::net::TcpStream;
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
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.peer_addr()?.as_socket().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get peer address",
        ))
    }

    /// Returns the socket address of the local half of this TCP connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{IpAddr, Ipv4Addr};
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// assert_eq!(stream.local_addr().unwrap().ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    /// # Ok(())
    /// # }
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_socket().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    pub fn set_mark(&self, mark: u32) -> io::Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_mark(mark)
    }

    pub fn mark(&self) -> io::Result<u32> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.mark()
    }

    /// Gets the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() {
    /// let stream = TcpStream::connect("127.0.0.1:8080")
    ///                        .await.expect("Couldn't connect to the server...");
    /// stream.take_error().expect("No error was expected...");
    /// # }
    /// ```
    pub fn take_error(&self) -> io::Result<Option<Error>> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.take_error()
    }
}

impl AsFd for Stream {
    #[inline(always)]
    fn as_raw_fd(&self) -> Fd {
        self.fd
    }
}

impl Into<std::os::unix::net::UnixStream> for Stream {
    fn into(self) -> std::os::unix::net::UnixStream {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) }
    }
}

impl From<std::os::unix::net::UnixStream> for Stream {
    fn from(stream: std::os::unix::net::UnixStream) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl From<Fd> for Stream {
    fn from(fd: Fd) -> Self {
        Self { fd }
    }
}

impl AsyncPollFd for Stream {}

impl AsyncShutdown for Stream {}

impl AsyncClose for Stream {}

impl Drop for Stream {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future.await.expect("Failed to close TCP stream");
        });
    }
}


use std::io::{Error, Result};
use std::net::{SocketAddr, ToSocketAddrs};
#[cfg(unix)]
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::time::{Duration, Instant};
use std::{io, mem};

use crate::io::recv::{Recv, RecvWithDeadline};
use crate::io::sys::{AsFd, Fd};
use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown, Bind};
use crate::runtime::local_executor;
use crate::{generate_peek, generate_recv, generate_send};

#[derive(Debug)]
pub struct ConnectedSocket {
    fd: Fd,
}

impl ConnectedSocket {
    #[inline(always)]
    #[cfg(unix)]
    pub fn borrow_fd(&self) -> BorrowedFd {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }

    generate_send!();

    generate_recv!();

    generate_peek!();

    /// Returns the socket address of the remote peer this socket was connected to.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use orengine::io::Bind;
    /// use orengine::net::UdpSocket;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// let connected_socket = socket.connect("192.168.0.1:41203").await.expect("couldn't connect to address");
    /// assert_eq!(connected_socket.peer_addr().unwrap(),
    ///            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 41203)));
    /// # Ok(())
    /// # }
    /// ```
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.peer_addr()?.as_socket().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get peer address",
        ))
    }

    /// Returns the socket address that this socket was created from.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use orengine::io::Bind;
    /// use orengine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// assert_eq!(socket.local_addr().unwrap(),
    ///            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 34254)));
    /// ```
    pub fn local_addr(&self) -> Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_socket().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::io::Bind;
    /// use orengine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_ttl(42).expect("set_ttl call failed");
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_ttl(ttl)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`ConnectedSocket::set_ttl`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::io::Bind;
    /// use orengine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_ttl(42).expect("set_ttl call failed");
    /// assert_eq!(socket.ttl().unwrap(), 42);
    /// ```
    pub fn ttl(&self) -> Result<u32> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.ttl()
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
    /// use orengine::io::Bind;
    /// use orengine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// match socket.take_error() {
    ///     Ok(Some(error)) => println!("UdpSocket error: {error:?}"),
    ///     Ok(None) => println!("No error"),
    ///     Err(error) => println!("UdpSocket.take_error failed: {error:?}"),
    /// }
    /// ```
    pub fn take_error(&self) -> Result<Option<Error>> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.take_error()
    }
}

impl Into<std::net::UdpSocket> for ConnectedSocket {
    fn into(self) -> std::net::UdpSocket {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::net::UdpSocket::from_raw_fd(fd) }
    }
}

impl From<std::net::UdpSocket> for ConnectedSocket {
    fn from(stream: std::net::UdpSocket) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl From<Fd> for ConnectedSocket {
    fn from(fd: Fd) -> Self {
        Self { fd }
    }
}

impl AsFd for ConnectedSocket {
    #[inline(always)]
    fn as_raw_fd(&self) -> Fd {
        self.fd
    }
}

impl AsyncPollFd for ConnectedSocket {}

impl AsyncShutdown for ConnectedSocket {}

impl AsyncClose for ConnectedSocket {}

impl Drop for ConnectedSocket {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future
                .await
                .expect("Failed to close UDP connected socket");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use crate::io::Bind;
    use crate::net::udp::Socket;
    use crate::runtime::create_local_executer_for_block_on;

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test]
    fn test_client() {
        const SERVER_ADDR: &str = "127.0.0.1:11086";
        const CLIENT_ADDR: &str = "127.0.0.1:11091";

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            let socket = std::net::UdpSocket::bind(SERVER_ADDR).expect("std bind failed");

            {
                let (is_ready_mu, condvar) = &*is_server_ready;
                let mut is_ready = is_ready_mu.lock().unwrap();
                *is_ready = true;
                condvar.notify_one();
            }

            let mut buf = vec![0u8; REQUEST.len()];

            for _ in 0..TIMES {
                let (n, src) = socket.recv_from(&mut buf).expect("accept failed");
                assert_eq!(REQUEST, &buf[..n]);

                socket.send_to(RESPONSE, &src).expect("std write failed");
            }
        });

        create_local_executer_for_block_on(async move {
            let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
            let mut is_server_ready = is_server_ready_mu.lock().unwrap();
            while *is_server_ready == false {
                is_server_ready = condvar.wait(is_server_ready).unwrap();
            }

            let stream = Socket::bind(CLIENT_ADDR).expect("bind failed");
            let mut connected_stream = stream.connect(SERVER_ADDR).await.expect("connect failed");

            assert_eq!(
                connected_stream.local_addr().expect(CLIENT_ADDR),
                SocketAddr::from_str(CLIENT_ADDR).unwrap()
            );
            assert_eq!(
                connected_stream.peer_addr().expect(CLIENT_ADDR),
                SocketAddr::from_str(SERVER_ADDR).unwrap()
            );

            for _ in 0..TIMES {
                connected_stream.send(REQUEST).await.expect("send failed");
                let mut buf = vec![0u8; RESPONSE.len()];

                connected_stream.recv(&mut buf).await.expect("recv failed");
                assert_eq!(RESPONSE, buf);
            }
        });

        server_thread.join().expect("server thread join failed");
    }

    #[test]
    fn test_timeout() {
        const ADDR: &str = "127.0.0.1:11141";
        const TIMEOUT: Duration = Duration::from_micros(1);

        create_local_executer_for_block_on(async {
            let socket = Socket::bind(ADDR).expect("bind failed");
            let mut connected_socket = socket
                .connect_with_timeout("127.0.0.1:14142", TIMEOUT)
                .await
                .expect("bind failed");

            match connected_socket.poll_recv_with_timeout(TIMEOUT).await {
                Ok(_) => panic!("poll_recv should timeout"),
                Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
            }

            match connected_socket
                .recv_with_timeout(&mut vec![0u8; 10], TIMEOUT)
                .await
            {
                Ok(_) => panic!("recv_from should timeout"),
                Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
            }

            match connected_socket
                .peek_with_timeout(&mut vec![0u8; 10], TIMEOUT)
                .await
            {
                Ok(_) => panic!("peek_from should timeout"),
                Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
            }
        });
    }
}

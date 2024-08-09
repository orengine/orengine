use std::io::{Error, Result};
use std::net::ToSocketAddrs;
#[cfg(unix)]
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::{io, mem};

use crate::io::sys::{AsFd, Fd};
use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown, Bind};
use crate::runtime::local_executor;
use crate::{
    generate_peek, generate_peek_exact, generate_recv, generate_recv_exact, generate_send,
    generate_send_all,
};

#[derive(Debug)]
pub struct ConnectedSocketDatagram {
    fd: Fd,
}

impl ConnectedSocketDatagram {
    #[inline(always)]
    #[cfg(unix)]
    pub fn borrow_fd(&self) -> BorrowedFd {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }

    generate_send!();

    generate_send_all!();

    generate_recv!();

    generate_recv_exact!();

    generate_peek!();

    generate_peek_exact!();

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
    pub fn peer_addr(&self) -> Result<std::os::unix::net::SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.peer_addr()?.as_unix().ok_or(Error::new(
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
    pub fn local_addr(&self) -> Result<std::os::unix::net::SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_unix().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    // TODO macro for it

    #[inline(always)]
    pub fn set_mark(&self, mark: u32) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_mark(mark)
    }

    #[inline(always)]
    pub fn mark(&self) -> Result<u32> {
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

impl Into<std::os::unix::net::UnixDatagram> for ConnectedSocketDatagram {
    fn into(self) -> std::os::unix::net::UnixDatagram {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::os::unix::net::UnixDatagram::from_raw_fd(fd) }
    }
}

impl From<std::os::unix::net::UnixDatagram> for ConnectedSocketDatagram {
    fn from(stream: std::os::unix::net::UnixDatagram) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl From<Fd> for ConnectedSocketDatagram {
    fn from(fd: Fd) -> Self {
        Self { fd }
    }
}

impl AsFd for ConnectedSocketDatagram {
    #[inline(always)]
    fn as_raw_fd(&self) -> Fd {
        self.fd
    }
}

impl AsyncPollFd for ConnectedSocketDatagram {}

impl AsyncShutdown for ConnectedSocketDatagram {}

impl AsyncClose for ConnectedSocketDatagram {}

impl Drop for ConnectedSocketDatagram {
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
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use crate::fs::test_helper::delete_file_if_exists;
    use crate::io::Bind;
    use crate::net::unix::socket_datagram::SocketDatagram;
    use crate::runtime::create_local_executer_for_block_on;

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test]
    fn test_client() {
        const SERVER_ADDR: &str = "/tmp/test_connected_socket_datagram_server.sock";
        const CLIENT_ADDR: &str = "/tmp/test_connected_socket_datagram_client.sock";

        delete_file_if_exists(SERVER_ADDR);
        delete_file_if_exists(CLIENT_ADDR);

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            let socket =
                std::os::unix::net::UnixDatagram::bind(SERVER_ADDR).expect("std bind failed");

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

                socket
                    .send_to_addr(RESPONSE, &src)
                    .expect("std write failed");
            }
        });

        create_local_executer_for_block_on(async move {
            let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
            let mut is_server_ready = is_server_ready_mu.lock().unwrap();
            while *is_server_ready == false {
                is_server_ready = condvar.wait(is_server_ready).unwrap();
            }

            let stream = SocketDatagram::bind(CLIENT_ADDR).expect("bind failed");
            let mut connected_stream = stream.connect(SERVER_ADDR).await.expect("connect failed");

            assert_eq!(
                connected_stream
                    .local_addr()
                    .expect(CLIENT_ADDR)
                    .as_pathname()
                    .unwrap(),
                Path::new(CLIENT_ADDR)
            );
            assert_eq!(
                connected_stream
                    .peer_addr()
                    .expect(CLIENT_ADDR)
                    .as_pathname()
                    .unwrap(),
                Path::new(SERVER_ADDR)
            );

            for _ in 0..TIMES {
                connected_stream
                    .send_all(REQUEST)
                    .await
                    .expect("send failed");
                let mut buf = vec![0u8; RESPONSE.len()];

                connected_stream.recv(&mut buf).await.expect("recv failed");
                assert_eq!(RESPONSE, buf);
            }
        });

        server_thread.join().expect("server thread join failed");
    }
}

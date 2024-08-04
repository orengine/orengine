use std::io::Error;
use std::net::{ToSocketAddrs};
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::time::{Duration, Instant};
use std::{io, mem};
use std::os::unix::net::SocketAddr;
use std::path::Path;

use socket2::{Domain, SockAddr, Socket, Type};

use crate::io::sys::{AsFd, Fd};
use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown, Connect, ConnectWithTimeout};
use crate::net::creators_of_sockets::new_unix_socket;
use crate::runtime::local_executor;
use crate::{
    generate_peek, generate_peek_exact, generate_recv,
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

    // region connect

    #[inline(always)]
    pub async fn connect<P: AsRef<Path>>(addr: P) -> io::Result<Self> {
        let socket = new_unix_socket()?;
        Connect::new(socket.into_raw_fd(), SockAddr::unix(addr)?).await
    }

    #[inline(always)]
    pub async fn connect_with_deadline<P: AsRef<Path>>(
        addr: P,
        deadline: Instant,
    ) -> io::Result<Self> {
        let socket = new_unix_socket()?;
        ConnectWithTimeout::new(socket.into_raw_fd(), SockAddr::unix(addr)?, deadline).await
    }

    #[inline(always)]
    pub async fn connect_with_timeout<P: AsRef<Path>>(
        addr: P,
        timeout: Duration,
    ) -> io::Result<Self> {
        Self::connect_with_deadline(addr, Instant::now() + timeout).await
    }

    // endregion

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
        socket_ref.peer_addr()?.as_unix().ok_or(Error::new(
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
        socket_ref.local_addr()?.as_unix().ok_or(Error::new(
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};
    use crate::fs::test_helper::{delete_file_if_exists};

    use crate::io::{Bind};
    use crate::net::unix::Listener;
    use crate::runtime::create_local_executer_for_block_on;
    use crate::sync::cond_var::LocalCondVar;
    use crate::sync::{LocalMutex, LocalWaitGroup};

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test]
    fn test_client() {
        const ADDR: &str = "/tmp/test_client.sock";

        delete_file_if_exists(ADDR);

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            use std::io::{Read, Write};
            let listener = std::os::unix::net::UnixListener::bind(ADDR).expect("std bind failed");

            {
                let (is_ready_mu, condvar) = &*is_server_ready;
                let mut is_ready = is_ready_mu.lock().unwrap();
                *is_ready = true;
                condvar.notify_one();
            }

            let mut stream = listener.accept().expect("accept failed").0;

            for _ in 0..TIMES {
                let mut buf = vec![0u8; REQUEST.len()];
                stream.read_exact(&mut buf).expect("std read failed");
                assert_eq!(REQUEST, buf);

                stream.write_all(RESPONSE).expect("std write failed");
            }
        });

        create_local_executer_for_block_on(async move {
            let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
            let mut is_server_ready = is_server_ready_mu.lock().unwrap();
            while *is_server_ready == false {
                is_server_ready = condvar.wait(is_server_ready).unwrap();
            }

            let mut stream = Stream::connect(ADDR).await.expect("connect failed");

            for _ in 0..TIMES {
                stream.send_all(REQUEST).await.expect("send failed");

                stream.poll_recv().await.expect("poll failed");
                let mut buf = vec![0u8; RESPONSE.len()];
                stream.recv_exact(&mut buf).await.expect("recv failed");
                assert_eq!(RESPONSE, buf);
            }
        });

        server_thread.join().expect("server thread join failed");
    }

    #[test]
    fn test_server() {
        const ADDR: &str = "/tmp/test_server.sock";

        delete_file_if_exists(ADDR);

        let is_server_ready = Arc::new(AtomicBool::new(false));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            create_local_executer_for_block_on(async move {
                let mut listener = Listener::bind(ADDR).expect("bind failed");

                is_server_ready_server_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                let mut stream = listener.accept().await.expect("accept failed");

                for _ in 0..TIMES {
                    stream.poll_recv().await.expect("poll failed");
                    let mut buf = vec![0u8; REQUEST.len()];
                    stream.recv_exact(&mut buf).await.expect("recv failed");
                    assert_eq!(REQUEST, buf);

                    stream.send_all(RESPONSE).await.expect("send failed");
                }
            });
        });

        use std::io::{Read, Write};
        while is_server_ready.load(std::sync::atomic::Ordering::Relaxed) == false {
            thread::sleep(Duration::from_millis(1));
        }

        let mut stream = std::os::unix::net::UnixStream::connect(ADDR).expect("connect failed");

        for _ in 0..TIMES {
            stream.write_all(REQUEST).expect("send failed");

            let mut buf = vec![0u8; RESPONSE.len()];
            stream.read_exact(&mut buf).expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    #[test]
    fn test_stream() {
        const ADDR: &str = "/tmp/test_stream.sock";

        delete_file_if_exists(ADDR);

        create_local_executer_for_block_on(async {
            let wg = LocalWaitGroup::new();
            wg.inc();
            let wg_clone = wg.clone();

            local_executor().spawn_local(async move {
                let mut listener = Listener::bind(ADDR).expect("bind failed");

                wg_clone.done();

                let mut stream = listener.accept().await.expect("accept failed");

                for _ in 0..TIMES {
                    stream.poll_recv().await.expect("poll failed");
                    let mut buf = vec![0u8; REQUEST.len()];

                    stream.peek_exact(&mut buf).await.expect("peek failed");
                    assert_eq!(REQUEST, buf);
                    stream.peek_exact(&mut buf).await.expect("peek failed");
                    assert_eq!(REQUEST, buf);

                    stream.recv_exact(&mut buf).await.expect("recv failed");
                    assert_eq!(REQUEST, buf);

                    stream.send_all(RESPONSE).await.expect("send failed");
                }
            });

            wg.wait().await;

            let mut stream = Stream::connect_with_timeout(ADDR, Duration::from_secs(2))
                .await
                .expect("connect with timeout failed");

            for _ in 0..TIMES {
                stream.poll_send().await.expect("poll failed");
                stream
                    .send_all_with_timeout(REQUEST, Duration::from_secs(2))
                    .await
                    .expect("send with timeout failed");

                stream
                    .poll_recv_with_timeout(Duration::from_secs(2))
                    .await
                    .expect("poll with timeout failed");
                let mut buf = vec![0u8; RESPONSE.len()];
                stream
                    .peek_with_timeout(&mut buf, Duration::from_secs(2))
                    .await
                    .expect("peek with timeout failed");
                stream
                    .peek_with_timeout(&mut buf, Duration::from_secs(2))
                    .await
                    .expect("peek with timeout failed");
                stream
                    .recv_with_timeout(&mut buf, Duration::from_secs(2))
                    .await
                    .expect("recv with timeout failed");
                assert_eq!(RESPONSE, buf);
            }
        });
    }
}
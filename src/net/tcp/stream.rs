//! This module contains [`Stream`].
use std::intrinsics::unlikely;
use std::io::{self, Error, Result};
use std::mem;
use std::net::{SocketAddr, ToSocketAddrs};
#[cfg(unix)]
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::time::{Duration, Instant};
use socket2::{Protocol, Type};
use crate::{each_addr, generate_peek, generate_peek_exact, generate_recv, generate_recv_exact, generate_send, generate_send_all};
use crate::io::{AsyncClose, AsyncPollFd};
use crate::io::connect::{Connect, ConnectWithTimeout};
use crate::io::recv::{Recv, RecvWithDeadline};
use crate::io::send::{Send, SendWithDeadline};
use crate::io::shutdown::AsyncShutdown;
use crate::io::sys::{AsFd, Fd};
use crate::net::get_socket::get_socket;
use crate::runtime::local_executor;

/// A TCP stream between a local and a remote socket.
///
/// # Close
///
/// [`Stream`] is automatically closed after it is dropped.
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
    pub async fn connect<A: ToSocketAddrs>(addrs: A) -> Result<Self> {
        each_addr!(&addrs, async move |addr: SocketAddr| -> Result<Self> {
            let socket = get_socket(addr, Type::STREAM, Some(Protocol::TCP))?;
            Connect::new(socket.into_raw_fd(), addr).await
        })
    }

    #[inline(always)]
    pub async fn connect_with_deadline<A: ToSocketAddrs>(addrs: A, deadline: Instant) -> Result<Self> {
        each_addr!(&addrs, async move |addr: SocketAddr| -> Result<Stream> {
            let socket = get_socket(addr, Type::STREAM, Some(Protocol::TCP))?;
            ConnectWithTimeout::new(socket.into_raw_fd(), addr, deadline).await
        })
    }

    #[inline(always)]
    pub async fn connect_with_timeout<A: ToSocketAddrs>(addrs: A, timeout: Duration) -> Result<Self> {
        Self::connect_with_deadline(addrs, Instant::now() + timeout).await
    }

    // endregion

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
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.peer_addr()?.as_socket().ok_or(Error::new(io::ErrorKind::Other, "failed to get peer address"))
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
    pub fn local_addr(&self) -> Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_socket().ok_or(Error::new(io::ErrorKind::Other, "failed to get local address"))
    }

    /// Sets the value of the `SO_LINGER` option on this socket.
    ///
    /// This value controls how the socket is closed when data remains
    /// to be sent. If `SO_LINGER` is set, the socket will remain open
    /// for the specified duration as the system attempts to send pending data.
    /// Otherwise, the system may close the socket immediately, or wait for a
    /// default timeout.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() {
    /// let stream = TcpStream::connect("127.0.0.1:8080")
    ///                        .await.expect("Couldn't connect to the server...");
    /// stream.set_linger(Some(Duration::from_secs(0))).expect("set_linger call failed");
    /// # }
    /// ```
    pub fn set_linger(&self, linger: Option<Duration>) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_linger(linger)
    }

    /// Gets the value of the `SO_LINGER` option on this socket.
    ///
    /// For more information about this option, see [`set_linger`](#method.set_linger).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() {
    /// let stream = TcpStream::connect("127.0.0.1:8080")
    ///                        .await.expect("Couldn't connect to the server...");
    /// stream.set_linger(Some(Duration::from_secs(0))).expect("set_linger call failed");
    /// assert_eq!(stream.linger().unwrap(), Some(Duration::from_secs(0)));
    /// # }
    /// ```
    pub fn linger(&self) -> Result<Option<Duration>> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.linger()
    }

    /// Sets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// If set, this option disables the Nagle algorithm. This means that
    /// segments are always sent as soon as possible, even if there is only a
    /// small amount of data. When not set, data is buffered until there is a
    /// sufficient amount to send out, thereby avoiding the frequent sending of
    /// small packets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() {
    /// let stream = TcpStream::connect("127.0.0.1:8080")
    ///                        .await.expect("Couldn't connect to the server...");
    /// stream.set_nodelay(true).expect("set_nodelay call failed");
    /// # }
    /// ```
    pub fn set_nodelay(&self, nodelay: bool) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_nodelay(nodelay)
    }

    /// Gets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// For more information about this option, see [`set_nodelay`](#method.set_nodelay).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() {
    /// let stream = TcpStream::connect("127.0.0.1:8080")
    ///                        .await.expect("Couldn't connect to the server...");
    /// stream.set_nodelay(true).expect("set_nodelay call failed");
    /// assert_eq!(stream.nodelay().unwrap_or(false), true);
    /// # }
    /// ```
    pub fn nodelay(&self) -> Result<bool> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.nodelay()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() {
    /// let stream = TcpStream::connect("127.0.0.1:8080")
    ///                        .await.expect("Couldn't connect to the server...");
    /// stream.set_ttl(100).expect("set_ttl call failed");
    /// # }
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_ttl(ttl)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`](#method.set_ttl).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() {
    /// let stream = TcpStream::connect("127.0.0.1:8080")
    ///                        .await.expect("Couldn't connect to the server...");
    /// stream.set_ttl(100).expect("set_ttl call failed");
    /// assert_eq!(stream.ttl().unwrap_or(0), 100);
    /// # }
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
    /// use orengine::net::TcpStream;
    ///
    /// # async fn foo() {
    /// let stream = TcpStream::connect("127.0.0.1:8080")
    ///                        .await.expect("Couldn't connect to the server...");
    /// stream.take_error().expect("No error was expected...");
    /// # }
    /// ```
    pub fn take_error(&self) -> Result<Option<Error>> {
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

impl Into<std::net::TcpStream> for Stream {
    fn into(self) -> std::net::TcpStream {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::net::TcpStream::from_raw_fd(fd) }
    }
}

impl From<std::net::TcpStream> for Stream {
    fn from(stream: std::net::TcpStream) -> Self {
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
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::AtomicBool;
    use std::thread;
    use crate::io::{AsyncAccept, Bind};
    use crate::io::bind::BindConfig;
    use crate::net::tcp::Listener;
    use super::*;
    use crate::runtime::create_local_executer_for_block_on;
    use crate::sync::{LocalMutex, LocalWaitGroup};
    use crate::sync::cond_var::LocalCondVar;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test]
    fn test_client() {
        const ADDR: &str = "127.0.0.1:6086";

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            use std::io::{Read, Write};
            let listener = std::net::TcpListener::bind(ADDR).expect("std bind failed");

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
        const ADDR: &str = "127.0.0.1:6081";

        let is_server_ready = Arc::new(AtomicBool::new(false));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            create_local_executer_for_block_on(async move {
                let mut listener = Listener::bind(ADDR).expect("bind failed");

                is_server_ready_server_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                let mut stream = listener.accept().await.expect("accept failed").0;

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

        let mut stream = std::net::TcpStream::connect(ADDR).expect("connect failed");

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
        const ADDR: &str = "127.0.0.1:6082";

        create_local_executer_for_block_on(async {
            let wg = LocalWaitGroup::new();
            wg.inc();
            let wg_clone = wg.clone();

            local_executor().spawn_local(async move {
                let mut listener = Listener::bind(ADDR).expect("bind failed");

                wg_clone.done();

                let mut stream = listener.accept().await.expect("accept failed").0;

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

            stream.set_ttl(133).expect("set_ttl failed");
            assert_eq!(stream.ttl().expect("get_ttl failed"), 133);

            stream.set_nodelay(true).expect("set_nodelay failed");
            assert_eq!(stream.nodelay().expect("get_nodelay failed"), true);
            stream.set_nodelay(false).expect("set_nodelay failed");
            assert_eq!(stream.nodelay().expect("get_nodelay failed"), false);

            stream.set_linger(Some(Duration::from_secs(23))).expect("set_linger failed");
            assert_eq!(stream.linger().expect("get_linger failed"), Some(Duration::from_secs(23)));

            for _ in 0..TIMES {
                stream.poll_send().await.expect("poll failed");
                stream.send_all_with_timeout(REQUEST, Duration::from_secs(2)).await.expect("send with timeout failed");

                stream.poll_recv_with_timeout(Duration::from_secs(2)).await.expect("poll with timeout failed");
                let mut buf = vec![0u8; RESPONSE.len()];
                stream.peek_with_timeout(&mut buf, Duration::from_secs(2)).await.expect("peek with timeout failed");
                stream.peek_with_timeout(&mut buf, Duration::from_secs(2)).await.expect("peek with timeout failed");
                stream.recv_with_timeout(&mut buf, Duration::from_secs(2)).await.expect("recv with timeout failed");
                assert_eq!(RESPONSE, buf);
            }
        });
    }

    #[test]
    fn test_timeout() {
        const ADDR: &str = "127.0.0.1:6083";
        const BACKLOG_SIZE: usize = 256;

        const CONNECT: usize = 0;
        const SEND: usize = 1;
        const POLL: usize = 2;
        const RECV: usize = 3;
        const PEEK: usize = 4;
        const TIMEOUT: Duration = Duration::from_millis(1);

        create_local_executer_for_block_on(async {
            let state = LocalMutex::new(CONNECT);
            let state_cond_var = LocalCondVar::new();
            let state_clone = state.clone();
            let state_cond_var_clone = state_cond_var.clone();
            let wg = LocalWaitGroup::new();
            wg.inc();
            let wg_clone = wg.clone();

            local_executor().spawn_local(async move {
                let mut listener = Listener::bind_with_config(
                    ADDR,
                    BindConfig::new().backlog_size(BACKLOG_SIZE)
                ).expect("bind failed");
                let mut expected_state = 0;
                let mut state = state_clone.lock().await;

                wg_clone.done();

                loop {
                    while *state != expected_state {
                        state = state_cond_var_clone.wait(state).await;
                    }
                    match *state {
                        CONNECT => {}
                        SEND => {
                            let _ = listener.accept().await.expect("accept failed").0;
                        }
                        POLL | PEEK | RECV => {
                            let _ = listener.accept().await.expect("accept failed").0;
                        }
                        _ => break
                    }
                    expected_state += 1;
                }
            });

            wg.wait().await;

            loop {
                let mut state = state.lock().await;
                match *state {
                    CONNECT => {
                        for _ in 0..BACKLOG_SIZE + 1 {
                            let _ = Stream::connect_with_timeout(ADDR, TIMEOUT)
                                .await
                                .expect("connect with timeout failed");
                        }
                        let res = Stream::connect_with_timeout(ADDR, TIMEOUT).await;
                        match res {
                            Ok(_) => panic!("connect with timeout should failed"),
                            Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                                panic!("connect with timeout should failed with TimedOut, but got {:?}", err)
                            }
                            Err(_) => {}
                        }
                    }

                    SEND => {
                        let mut stream = Stream::connect_with_timeout(ADDR, TIMEOUT)
                            .await
                            .expect("connect with timeout failed");

                        let buf = vec![0u8; 1 << 24]; // 1 MB.
                        // It is impossible to send 1 MB in 1 microsecond (1 TB/s).
                        let res = stream.send_all_with_deadline(
                            &buf,
                            Instant::now() + Duration::from_micros(1)
                        ).await;
                        match res {
                            Ok(_) => panic!("send with timeout should failed"),
                            Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                                panic!("send with timeout should failed with TimedOut, but got {:?}", err)
                            }
                            Err(_) => {}
                        }
                    }

                    POLL => {
                        let stream = Stream::connect_with_timeout(ADDR, TIMEOUT)
                            .await
                            .expect("connect with timeout failed");

                        let res = stream.poll_recv_with_timeout(TIMEOUT).await;
                        match res {
                            Ok(_) => panic!("poll with timeout should failed"),
                            Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                                panic!("poll with timeout should failed with TimedOut, but got {:?}", err)
                            }
                            Err(_) => {}
                        }
                    }

                    RECV => {
                        let mut stream = Stream::connect_with_timeout(ADDR, TIMEOUT)
                            .await
                            .expect("connect with timeout failed");

                        let mut buf = vec![0u8; REQUEST.len()];
                        let res = stream.recv_with_timeout(&mut buf, TIMEOUT).await;
                        match res {
                            Ok(_) => panic!("recv with timeout should failed"),
                            Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                                panic!("recv with timeout should failed with TimedOut, but got {:?}", err)
                            }
                            Err(_) => {}
                        }
                    }

                    PEEK => {
                        let mut stream = Stream::connect_with_timeout(ADDR, TIMEOUT)
                            .await
                            .expect("connect with timeout failed");

                        let mut buf = vec![0u8; REQUEST.len()];
                        let res = stream.peek_with_timeout(&mut buf, TIMEOUT).await;
                        match res {
                            Ok(_) => panic!("peek with timeout should failed"),
                            Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                                panic!("peek with timeout should failed with TimedOut, but got {:?}", err)
                            }
                            Err(_) => {}
                        }
                    }

                    _ => break
                }
                *state += 1;
                state_cond_var.notify_one();
            }
        });
    }
}
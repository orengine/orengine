use std::ffi::c_int;
use std::io::{Error, Result};
use std::{io, mem};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
#[cfg(unix)]
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::time::{Duration, Instant};
use socket2::{Protocol, SockAddr, Type};
use crate::{each_addr, each_addr_sync, generate_peek, generate_peek_from, generate_recv, generate_recv_from, generate_send, generate_send_to};
use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown, Bind};
use crate::io::bind::BindConfig;
use crate::io::connect::{Connect, ConnectWithTimeout};
use crate::io::recv::{Recv, RecvWithDeadline};
use crate::io::send::SendWithDeadline;
use crate::io::send_to::{SendTo, SendToWithDeadline};
use crate::io::sys::{AsFd, Fd};
use crate::net::get_socket::get_socket;
use crate::runtime::local_executor;
use crate::utils::addr_from_to_socket_addrs;

pub struct Socket {
    fd: Fd
}

impl Socket {
    #[inline(always)]
    #[cfg(unix)]
    pub fn borrow_fd(&self) -> BorrowedFd {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }

    // region connect

    #[inline(always)]
    pub async fn connect<A: ToSocketAddrs>(&self, addrs: A) -> Result<Self> {
        each_addr!(&addrs, async move |addr: SocketAddr| -> Result<Self> {
            Connect::new(self.fd, addr).await
        })
    }

    #[inline(always)]
    pub async fn connect_with_deadline<A: ToSocketAddrs>(&self, addrs: A, deadline: Instant) -> Result<Self> {
        each_addr!(&addrs, async move |addr: SocketAddr| -> Result<Self> {
            ConnectWithTimeout::new(self.fd, addr, deadline).await
        })
    }

    #[inline(always)]
    pub async fn connect_with_timeout<A: ToSocketAddrs>(&self, addrs: A, timeout: Duration) -> Result<Self> {
        self.connect_with_deadline(addrs, Instant::now() + timeout).await
    }

    // endregion

    generate_send!();

    generate_send_to!();

    generate_recv!();

    generate_recv_from!();

    generate_peek!();

    generate_peek_from!();

    /// Returns the socket address of the remote peer this socket was connected to.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.connect("192.168.0.1:41203").await.expect("couldn't connect to address");
    /// assert_eq!(socket.peer_addr().unwrap(),
    ///            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 41203)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// If the socket isn't connected, it will return a [`NotConnected`] error.
    ///
    /// [`NotConnected`]: io::ErrorKind::NotConnected
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// assert_eq!(socket.peer_addr().unwrap_err().kind(),
    ///            std::io::ErrorKind::NotConnected);
    /// ```
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.peer_addr()?.as_socket().ok_or(Error::new(io::ErrorKind::Other, "failed to get peer address"))
    }

    /// Returns the socket address that this socket was created from.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// assert_eq!(socket.local_addr().unwrap(),
    ///            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 34254)));
    /// ```
    pub fn local_addr(&self) -> Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_socket().ok_or(Error::new(io::ErrorKind::Other, "failed to get local address"))
    }

    /// Sets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// When enabled, this socket is allowed to send packets to a broadcast
    /// address.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_broadcast(false).expect("set_broadcast call failed");
    /// ```
    pub fn set_broadcast(&self, broadcast: bool) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_broadcast(broadcast)
    }

    /// Gets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// For more information about this option, see [`Socket::set_broadcast`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_broadcast(false).expect("set_broadcast call failed");
    /// assert_eq!(socket.broadcast().unwrap(), false);
    /// ```
    pub fn broadcast(&self) -> Result<bool> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.broadcast()
    }

    /// Sets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// If enabled, multicast packets will be looped back to the local socket.
    /// Note that this might not have any effect on IPv6 sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_multicast_loop_v4(false).expect("set_multicast_loop_v4 call failed");
    /// ```
    pub fn set_multicast_loop_v4(&self, multicast_loop_v4: bool) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_multicast_loop_v4(multicast_loop_v4)
    }

    /// Gets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see [`Socket::set_multicast_loop_v4`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_multicast_loop_v4(false).expect("set_multicast_loop_v4 call failed");
    /// assert_eq!(socket.multicast_loop_v4().unwrap(), false);
    /// ```
    pub fn multicast_loop_v4(&self) -> Result<bool> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.multicast_loop_v4()
    }

    /// Sets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// Indicates the time-to-live value of outgoing multicast packets for
    /// this socket. The default value is 1 which means that multicast packets
    /// don't leave the local network unless explicitly requested.
    ///
    /// Note that this might not have any effect on IPv6 sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_multicast_ttl_v4(42).expect("set_multicast_ttl_v4 call failed");
    /// ```
    pub fn set_multicast_ttl_v4(&self, multicast_ttl_v4: u32) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_multicast_ttl_v4(multicast_ttl_v4)
    }

    /// Gets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// For more information about this option, see [`Socket::set_multicast_ttl_v4`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_multicast_ttl_v4(42).expect("set_multicast_ttl_v4 call failed");
    /// assert_eq!(socket.multicast_ttl_v4().unwrap(), 42);
    /// ```
    pub fn multicast_ttl_v4(&self) -> Result<u32> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.multicast_ttl_v4()
    }

    /// Sets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// Controls whether this socket sees the multicast packets it sends itself.
    /// Note that this might not have any affect on IPv4 sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_multicast_loop_v6(false).expect("set_multicast_loop_v6 call failed");
    /// ```
    pub fn set_multicast_loop_v6(&self, multicast_loop_v6: bool) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_multicast_loop_v6(multicast_loop_v6)
    }

    /// Gets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see [`Socket::set_multicast_loop_v6`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
    ///
    /// let socket = UdpSocket::bind("127.0.0.1:34254").expect("couldn't bind to address");
    /// socket.set_multicast_loop_v6(false).expect("set_multicast_loop_v6 call failed");
    /// assert_eq!(socket.multicast_loop_v6().unwrap(), false);
    /// ```
    pub fn multicast_loop_v6(&self) -> Result<bool> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.multicast_loop_v6()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
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
    /// For more information about this option, see [`Socket::set_ttl`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
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

    /// Executes an operation of the `IP_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// address of the local interface with which the system should join the
    /// multicast group. If it's equal to `INADDR_ANY` then an appropriate
    /// interface is chosen by the system.
    pub fn join_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.join_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// index of the interface to join/leave (or 0 to indicate any interface).
    pub fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.join_multicast_v6(multiaddr, interface)
    }

    /// Executes an operation of the `IP_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`Socket::join_multicast_v4`].
    pub fn leave_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.leave_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`Socket::join_multicast_v6`].
    pub fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.leave_multicast_v6(multiaddr, interface)
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
    /// use async_engine::io::Bind;
    /// use async_engine::net::UdpSocket;
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

impl Bind for Socket {
    fn bind_with_config<A: ToSocketAddrs>(addrs: A, config: BindConfig) -> Result<Self> {
        each_addr_sync!(&addrs, move |addr| {
            let socket = get_socket(addr, Type::DGRAM, None)?;
            if config.only_v6 {
                socket.set_only_v6(true)?;
            }

            if config.reuse_address {
               socket.set_reuse_address(true)?;
            }

            if config.reuse_port {
                socket.set_reuse_port(true)?;
            }

            socket.bind(&SockAddr::from(addr))?;
            Ok(Self {
                fd: socket.into_raw_fd()
            })
        })
    }
}

impl Into<std::net::UdpSocket> for Socket {
    fn into(self) -> std::net::UdpSocket {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::net::UdpSocket::from_raw_fd(fd) }
    }
}

impl From<std::net::UdpSocket> for Socket {
    fn from(stream: std::net::UdpSocket) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl From<Fd> for Socket {
    fn from(fd: Fd) -> Self {
        Self { fd }
    }
}

impl AsFd for Socket {
    #[inline(always)]
    fn as_raw_fd(&self) -> Fd {
        self.fd
    }
}

impl AsyncPollFd for Socket {}

impl AsyncShutdown for Socket {}

impl AsyncClose for Socket {}

impl Drop for Socket {
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
    use super::*;
    use crate::runtime::create_local_executer_for_block_on;
    use crate::sync::{LocalMutex, LocalWaitGroup};
    use crate::sync::cond_var::LocalCondVar;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test]
    fn test_client() {
        const SERVER_ADDR: &str = "127.0.0.1:10086";

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

            let mut stream = Socket::bind("127.0.0.1:9081").expect("bind failed");
            //stream.connect(SERVER_ADDR).await.expect("connect failed");

            for _ in 0..TIMES {
                stream.send_to(REQUEST, SERVER_ADDR).await.expect("send failed");
                let mut buf = vec![0u8; RESPONSE.len()];
                stream.recv_from(&mut buf).await.expect("recv failed");
                assert_eq!(RESPONSE, buf);
            }
        });

        server_thread.join().expect("server thread join failed");
    }

    #[test]
    fn test_server() {
        const SERVER_ADDR: &str = "127.0.0.1:10082";

        let is_server_ready = Arc::new(AtomicBool::new(false));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            create_local_executer_for_block_on(async move {
                let mut server = Socket::bind(SERVER_ADDR).expect("bind failed");

                is_server_ready_server_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                for _ in 0..TIMES {
                    server.poll_recv().await.expect("poll failed");
                    let mut buf = vec![0u8; REQUEST.len()];
                    let (n, src) = server.recv_from(&mut buf).await.expect("accept failed");
                    assert_eq!(REQUEST, buf);

                    server.send_to(RESPONSE, &src).await.expect("send failed");
                }
            });
        });

        while is_server_ready.load(std::sync::atomic::Ordering::Relaxed) == false {
            thread::sleep(Duration::from_millis(1));
        }

        let mut stream = std::net::UdpSocket::bind("127.0.0.1:9082").expect("connect failed");
        stream.connect(SERVER_ADDR).expect("connect failed");

        for _ in 0..TIMES {
            stream.send(REQUEST).expect("send failed");

            let mut buf = vec![0u8; RESPONSE.len()];
            stream.recv(&mut buf).expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    // #[test]
    // fn test_stream() {
    //     const ADDR: &str = "127.0.0.1:8082";
    //
    //     create_local_executer_for_block_on(async {
    //         let wg = LocalWaitGroup::new();
    //         wg.inc();
    //         let wg_clone = wg.clone();
    //
    //         local_executor().spawn_local(async move {
    //             let mut listener = Listener::bind(ADDR).expect("bind failed");
    //
    //             wg_clone.done();
    //
    //             let mut stream = listener.accept().await.expect("accept failed").0;
    //
    //             for _ in 0..TIMES {
    //                 stream.poll().await.expect("poll failed");
    //                 let mut buf = vec![0u8; REQUEST.len()];
    //
    //                 stream.peek_exact(&mut buf).await.expect("peek failed");
    //                 assert_eq!(REQUEST, buf);
    //                 stream.peek_exact(&mut buf).await.expect("peek failed");
    //                 assert_eq!(REQUEST, buf);
    //
    //                 stream.recv_exact(&mut buf).await.expect("recv failed");
    //                 assert_eq!(REQUEST, buf);
    //
    //                 stream.send_all(RESPONSE).await.expect("send failed");
    //             }
    //         });
    //
    //         wg.wait().await;
    //
    //         let mut stream = Stream::connect_with_timeout(ADDR, Duration::from_secs(2))
    //             .await
    //             .expect("connect with timeout failed");
    //
    //         stream.set_ttl(133).expect("set_ttl failed");
    //         assert_eq!(stream.ttl().expect("get_ttl failed"), 133);
    //
    //         stream.set_nodelay(true).expect("set_nodelay failed");
    //         assert_eq!(stream.nodelay().expect("get_nodelay failed"), true);
    //         stream.set_nodelay(false).expect("set_nodelay failed");
    //         assert_eq!(stream.nodelay().expect("get_nodelay failed"), false);
    //
    //         stream.set_linger(Some(Duration::from_secs(23))).expect("set_linger failed");
    //         assert_eq!(stream.linger().expect("get_linger failed"), Some(Duration::from_secs(23)));
    //
    //         for _ in 0..TIMES {
    //             stream.send_all_with_timeout(REQUEST, Duration::from_secs(2)).await.expect("send with timeout failed");
    //
    //             stream.poll_with_timeout(Duration::from_secs(2)).await.expect("poll with timeout failed");
    //             let mut buf = vec![0u8; RESPONSE.len()];
    //             stream.peek_with_timeout(&mut buf, Duration::from_secs(2)).await.expect("peek with timeout failed");
    //             stream.peek_with_timeout(&mut buf, Duration::from_secs(2)).await.expect("peek with timeout failed");
    //             stream.recv_with_timeout(&mut buf, Duration::from_secs(2)).await.expect("recv with timeout failed");
    //             assert_eq!(RESPONSE, buf);
    //         }
    //     });
    // }
    //
    // #[test]
    // fn test_timeout() {
    //     const ADDR: &str = "127.0.0.1:8083";
    //     const BACKLOG_SIZE: usize = 256;
    //
    //     const CONNECT: usize = 0;
    //     const SEND: usize = 1;
    //     const POLL: usize = 2;
    //     const RECV: usize = 3;
    //     const PEEK: usize = 4;
    //     const TIMEOUT: Duration = Duration::from_millis(1);
    //
    //     create_local_executer_for_block_on(async {
    //         let state = LocalMutex::new(CONNECT);
    //         let state_cond_var = LocalCondVar::new();
    //         let state_clone = state.clone();
    //         let state_cond_var_clone = state_cond_var.clone();
    //         let wg = LocalWaitGroup::new();
    //         wg.inc();
    //         let wg_clone = wg.clone();
    //
    //         local_executor().spawn_local(async move {
    //             let mut listener = Listener::bind_with_config(
    //                 ADDR,
    //                 BindConfig::new().backlog_size(BACKLOG_SIZE)
    //             ).expect("bind failed");
    //             let mut expected_state = 0;
    //             let mut state = state_clone.lock().await;
    //
    //             wg_clone.done();
    //
    //             loop {
    //                 while *state != expected_state {
    //                     state = state_cond_var_clone.wait(state).await;
    //                 }
    //                 match *state {
    //                     CONNECT => {}
    //                     SEND => {
    //                         let _ = listener.accept().await.expect("accept failed").0;
    //                     }
    //                     POLL | PEEK | RECV => {
    //                         let _ = listener.accept().await.expect("accept failed").0;
    //                     }
    //                     _ => break
    //                 }
    //                 expected_state += 1;
    //             }
    //         });
    //
    //         wg.wait().await;
    //
    //         loop {
    //             let mut state = state.lock().await;
    //             match *state {
    //                 CONNECT => {
    //                     for _ in 0..BACKLOG_SIZE + 1 {
    //                         let _ = Stream::connect_with_timeout(ADDR, TIMEOUT)
    //                             .await
    //                             .expect("connect with timeout failed");
    //                     }
    //                     let res = Stream::connect_with_timeout(ADDR, TIMEOUT).await;
    //                     match res {
    //                         Ok(_) => panic!("connect with timeout should failed"),
    //                         Err(err) if err.kind() != io::ErrorKind::TimedOut => {
    //                             panic!("connect with timeout should failed with TimedOut, but got {:?}", err)
    //                         }
    //                         Err(_) => {}
    //                     }
    //                 }
    //
    //                 SEND => {
    //                     let mut stream = Stream::connect_with_timeout(ADDR, TIMEOUT)
    //                         .await
    //                         .expect("connect with timeout failed");
    //
    //                     let buf = vec![0u8; 1 << 24]; // 1 MB.
    //                     // It is impossible to send 1 MB in 1 microsecond (1 TB/s).
    //                     let res = stream.send_all_with_deadline(
    //                         &buf,
    //                         Instant::now() + Duration::from_micros(1)
    //                     ).await;
    //                     match res {
    //                         Ok(_) => panic!("send with timeout should failed"),
    //                         Err(err) if err.kind() != io::ErrorKind::TimedOut => {
    //                             panic!("send with timeout should failed with TimedOut, but got {:?}", err)
    //                         }
    //                         Err(_) => {}
    //                     }
    //                 }
    //
    //                 POLL => {
    //                     let stream = Stream::connect_with_timeout(ADDR, TIMEOUT)
    //                         .await
    //                         .expect("connect with timeout failed");
    //
    //                     let res = stream.poll_with_timeout(TIMEOUT).await;
    //                     match res {
    //                         Ok(_) => panic!("poll with timeout should failed"),
    //                         Err(err) if err.kind() != io::ErrorKind::TimedOut => {
    //                             panic!("poll with timeout should failed with TimedOut, but got {:?}", err)
    //                         }
    //                         Err(_) => {}
    //                     }
    //                 }
    //
    //                 RECV => {
    //                     let mut stream = Stream::connect_with_timeout(ADDR, TIMEOUT)
    //                         .await
    //                         .expect("connect with timeout failed");
    //
    //                     let mut buf = vec![0u8; REQUEST.len()];
    //                     let res = stream.recv_with_timeout(&mut buf, TIMEOUT).await;
    //                     match res {
    //                         Ok(_) => panic!("recv with timeout should failed"),
    //                         Err(err) if err.kind() != io::ErrorKind::TimedOut => {
    //                             panic!("recv with timeout should failed with TimedOut, but got {:?}", err)
    //                         }
    //                         Err(_) => {}
    //                     }
    //                 }
    //
    //                 PEEK => {
    //                     let mut stream = Stream::connect_with_timeout(ADDR, TIMEOUT)
    //                         .await
    //                         .expect("connect with timeout failed");
    //
    //                     let mut buf = vec![0u8; REQUEST.len()];
    //                     let res = stream.peek_with_timeout(&mut buf, TIMEOUT).await;
    //                     match res {
    //                         Ok(_) => panic!("peek with timeout should failed"),
    //                         Err(err) if err.kind() != io::ErrorKind::TimedOut => {
    //                             panic!("peek with timeout should failed with TimedOut, but got {:?}", err)
    //                         }
    //                         Err(_) => {}
    //                     }
    //                 }
    //
    //                 _ => break
    //             }
    //             *state += 1;
    //             state_cond_var.notify_one();
    //         }
    //     });
    // }
}
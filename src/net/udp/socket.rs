use std::fmt::{Debug, Formatter};
use std::io::Result;
use std::mem::ManuallyDrop;
use std::net::SocketAddr;

use socket2::{SockAddr, SockRef};

use crate::io::sys::{AsRawSocket, AsSocket, FromRawSocket, IntoRawSocket, RawSocket};
use crate::io::{
    AsyncBind, AsyncConnectDatagram, AsyncPeekFrom, AsyncPollSocket, AsyncRecvFrom, AsyncSendTo,
    AsyncSocketClose,
};
use crate::net::creators_of_sockets::new_udp_socket;
use crate::net::udp::connected_socket::UdpConnectedSocket;
use crate::net::BindConfig;
use crate::net::{Datagram, Socket};
use crate::runtime::local_executor;

/// A UDP socket.
///
/// After creating a `UdpSocket` by [`bind`](UdpSocket::bind)ing it to a socket address, data can be
/// [sent to](AsyncSendTo) and [received from](AsyncRecvFrom) any other socket address.
///
/// Although UDP is a connectionless protocol, this implementation provides an interface
/// to set an address where data should be sent and received from.
/// [`UdpSocket::connect`](AsyncConnectDatagram)
/// returns [`UdpConnectedSocket`] which implements
/// [`ConnectedDatagram`](crate::net::connected_datagram::ConnectedDatagram),
/// [`AsyncRecv`](crate::io::AsyncRecv), [`AsyncPeek`](crate::io::AsyncPeek),
/// [`AsyncSend`](crate::io::AsyncSend).
///
/// # Examples
///
/// ## Usage without [`connect`](AsyncConnectDatagram)
///
/// ```rust
/// use orengine::net::UdpSocket;
/// use orengine::io::{full_buffer, AsyncBind, AsyncPollSocket, AsyncRecvFrom, AsyncSendTo};
///
/// # async fn foo() {
/// let mut socket = UdpSocket::bind("127.0.0.1:8081").await.unwrap();
/// loop {
///    socket.poll_recv().await.expect("poll failed");
///    let mut buf = full_buffer();
///    let (n, addr) = socket.recv_from(&mut buf).await.expect("recv_from failed");
///    if n == 0 {
///        continue;
///    }
///
///    socket.send_to(&buf[..n], addr).await.expect("send_to failed");
/// }
/// # }
/// ```
///
/// ## Usage with [`connect`](AsyncConnectDatagram)
///
/// ```rust
/// use orengine::io::{full_buffer, AsyncBind, AsyncConnectDatagram, AsyncPollSocket, AsyncRecv, AsyncSend};
/// use orengine::net::UdpSocket;
///
/// # async fn foo() {
/// let socket = UdpSocket::bind("127.0.0.1:8081").await.unwrap();
/// let mut connected_socket = socket.connect("127.0.0.1:8080").await.unwrap();
/// loop {
///    connected_socket.poll_recv().await.expect("poll failed");
///    let mut buf = full_buffer();
///    let n = connected_socket.recv(&mut buf).await.expect("recv_from failed");
///    if n == 0 {
///        break;
///    }
///
///    connected_socket.send(&buf.slice(..n)).await.expect("send_to failed");
/// }
/// # }
/// ```
pub struct UdpSocket {
    raw_socket: RawSocket,
}

impl From<UdpSocket> for std::net::UdpSocket {
    fn from(socket: UdpSocket) -> Self {
        unsafe { Self::from_raw_socket(ManuallyDrop::new(socket).raw_socket) }
    }
}

impl From<std::net::UdpSocket> for UdpSocket {
    fn from(stream: std::net::UdpSocket) -> Self {
        Self {
            raw_socket: stream.into_raw_socket(),
        }
    }
}

#[cfg(unix)]
impl std::os::fd::IntoRawFd for UdpSocket {
    fn into_raw_fd(self) -> std::os::fd::RawFd {
        ManuallyDrop::new(self).raw_socket
    }
}

#[cfg(windows)]
impl std::os::windows::io::IntoRawSocket for UdpSocket {
    fn into_raw_socket(self) -> RawSocket {
        ManuallyDrop::new(self).raw_socket
    }
}

impl IntoRawSocket for UdpSocket {}

#[cfg(unix)]
impl std::os::fd::AsRawFd for UdpSocket {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.raw_socket
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawSocket for UdpSocket {
    fn as_raw_socket(&self) -> RawSocket {
        self.raw_socket
    }
}

impl AsRawSocket for UdpSocket {}

#[cfg(unix)]
impl std::os::fd::AsFd for UdpSocket {
    fn as_fd(&self) -> std::os::fd::BorrowedFd {
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.raw_socket) }
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for UdpSocket {
    fn as_socket(&self) -> BorrowedSocket {
        unsafe { std::os::windows::io::BorrowedSocket::borrow_raw(self.raw_socket) }
    }
}

impl AsSocket for UdpSocket {}

#[cfg(unix)]
impl std::os::fd::FromRawFd for UdpSocket {
    unsafe fn from_raw_fd(raw_fd: std::os::fd::RawFd) -> Self {
        Self { raw_socket: raw_fd }
    }
}

#[cfg(windows)]
impl std::os::windows::io::FromRawSocket for UdpSocket {
    unsafe fn from_raw_socket(raw_socket: RawSocket) -> Self {
        Self { raw_socket }
    }
}

impl FromRawSocket for UdpSocket {}

impl AsyncBind for UdpSocket {
    async fn new_socket(addr: &SocketAddr) -> Result<RawSocket> {
        new_udp_socket(addr).await
    }

    fn bind_and_listen_if_needed(
        sock_ref: SockRef<'_>,
        addr: SocketAddr,
        _config: &BindConfig,
    ) -> Result<()> {
        sock_ref.bind(&SockAddr::from(addr))
    }
}

impl AsyncConnectDatagram<UdpConnectedSocket> for UdpSocket {}

impl AsyncPollSocket for UdpSocket {}

impl AsyncRecvFrom for UdpSocket {}

impl AsyncPeekFrom for UdpSocket {}

impl AsyncSendTo for UdpSocket {}

impl AsyncSocketClose for UdpSocket {}

impl Socket for UdpSocket {}

impl Datagram for UdpSocket {
    type ConnectedDatagram = UdpConnectedSocket;
}

impl Debug for UdpSocket {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("UdpSocket");

        if let Ok(addr) = self.local_addr() {
            res.field("local addr", &addr);
        }

        res.field("raw_socket", &AsRawSocket::as_raw_socket(self))
            .finish()
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_local_future(async {
            close_future.await.expect("Failed to close UDP socket");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::rc::Rc;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use std::{io, thread};

    use super::*;
    use crate as orengine;
    use crate::io::{AsyncBind, AsyncRecv, AsyncSend};
    use crate::net::ReusePort;
    use crate::runtime::local_executor;
    use crate::sync::{AsyncCondVar, AsyncMutex, LocalCondVar, LocalMutex};
    use crate::yield_now;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[orengine::test::test_local]
    fn test_udp_client() {
        const SERVER_ADDR: &str = "127.0.0.1:10086";

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            let socket = std::net::UdpSocket::bind(SERVER_ADDR).expect("std bind failed");

            {
                let (is_ready_mu, condvar) = &*is_server_ready;
                let mut is_ready = is_ready_mu.lock().unwrap();
                *is_ready = true;
                drop(is_ready);
                condvar.notify_one();
            }

            let mut buf = vec![0u8; REQUEST.len()];

            for _ in 0..TIMES {
                let (n, src) = socket.recv_from(&mut buf).expect("accept failed");
                assert_eq!(REQUEST, &buf[..n]);

                socket.send_to(RESPONSE, src).expect("std write failed");
            }
        });

        {
            let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;

            loop {
                let is_server_ready = is_server_ready_mu.lock().expect("lock failed");
                if *is_server_ready {
                    drop(is_server_ready);
                    break;
                }

                let _unused = condvar.wait(is_server_ready).expect("wait failed");
            }
        }

        let mut stream = UdpSocket::bind("127.0.0.1:9081")
            .await
            .expect("bind failed");

        for _ in 0..TIMES {
            stream
                .send_to(REQUEST, SERVER_ADDR)
                .await
                .expect("send failed");
            let mut buf = vec![0u8; RESPONSE.len()];

            stream.recv_from(&mut buf).await.expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    #[allow(clippy::future_not_send, reason = "It is a test")]
    async fn test_server_with_config(
        server_addr_str: String,
        client_addr_str: String,
        config: BindConfig,
    ) {
        let is_server_ready = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
        let is_server_ready_clone = is_server_ready.clone();
        let addr_clone = server_addr_str.clone();

        local_executor().spawn_local(async move {
            let mut server = UdpSocket::bind_with_config(addr_clone, &config)
                .await
                .expect("bind failed");

            *is_server_ready_clone.0.lock().await = true;
            is_server_ready_clone.1.notify_one();

            for _ in 0..TIMES {
                server
                    .poll_recv_with_timeout(Duration::from_secs(10))
                    .await
                    .expect("poll failed");
                let mut buf = vec![0u8; REQUEST.len()];
                let (n, src) = server
                    .recv_from_with_timeout(&mut buf, Duration::from_secs(10))
                    .await
                    .expect("accept failed");
                assert_eq!(REQUEST, &buf[..n]);

                server
                    .send_to_with_timeout(RESPONSE, &src, Duration::from_secs(10))
                    .await
                    .expect("send failed");
            }
        });

        let mut is_server_ready_guard = is_server_ready.0.lock().await;
        while !*is_server_ready_guard {
            is_server_ready_guard = is_server_ready.1.wait(is_server_ready_guard).await;
        }

        let mut socket = UdpSocket::bind(client_addr_str)
            .await
            .expect("connect failed")
            .connect_with_timeout(server_addr_str, Duration::from_secs(10))
            .await
            .expect("connect failed");

        for _ in 0..TIMES {
            socket
                .send_bytes_with_timeout(REQUEST, Duration::from_secs(10))
                .await
                .expect("send failed");

            let mut buf = vec![0u8; RESPONSE.len()];
            socket
                .recv_bytes_with_timeout(&mut buf, Duration::from_secs(10))
                .await
                .expect("recv failed");
        }

        yield_now().await;
        thread::yield_now();
    }

    #[orengine::test::test_local]
    fn test_server_without_reuse_port() {
        let config = BindConfig::default();
        test_server_with_config(
            "127.0.0.1:10037".to_string(),
            "127.0.0.1:9082".to_string(),
            config.reuse_port(ReusePort::Disabled),
        )
        .await;
    }

    #[orengine::test::test_local]
    fn test_server_with_default_reuse_port() {
        let config = BindConfig::default();
        test_server_with_config(
            "127.0.0.1:10038".to_string(),
            "127.0.0.1:9083".to_string(),
            config.reuse_port(ReusePort::Default),
        )
        .await;
    }

    #[orengine::test::test_local]
    fn test_server_with_cpu_reuse_port() {
        let config = BindConfig::default();
        test_server_with_config(
            "127.0.0.1:10039".to_string(),
            "127.0.0.1:9084".to_string(),
            config.reuse_port(ReusePort::CPU),
        )
        .await;
    }

    #[orengine::test::test_local]
    fn test_socket() {
        const SERVER_ADDR: &str = "127.0.0.1:10090";
        const CLIENT_ADDR: &str = "127.0.0.1:10091";
        const TIMEOUT: Duration = Duration::from_secs(3);

        let is_server_ready = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        local_executor().exec_local_future(async move {
            let mut server = UdpSocket::bind(SERVER_ADDR).await.expect("bind failed");

            {
                let (is_ready_mu, condvar) = &*is_server_ready;
                let mut is_ready = is_ready_mu.lock().await;
                *is_ready = true;
                condvar.notify_one();
            }

            for _ in 0..TIMES {
                server
                    .poll_recv_with_timeout(TIMEOUT)
                    .await
                    .expect("poll failed");
                let mut buf = vec![0u8; REQUEST.len()];
                let (n, src) = server
                    .recv_from_with_timeout(&mut buf, TIMEOUT)
                    .await
                    .expect("accept failed");
                assert_eq!(REQUEST, &buf[..n]);

                server
                    .send_to_with_timeout(RESPONSE, &src, TIMEOUT)
                    .await
                    .expect("send failed");
            }
        });

        {
            let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
            let mut is_server_ready = is_server_ready_mu.lock().await;
            while !(*is_server_ready) {
                is_server_ready = condvar.wait(is_server_ready).await;
            }
        }

        let mut stream = UdpSocket::bind(CLIENT_ADDR).await.expect("bind failed");

        assert_eq!(
            stream.local_addr().expect("Failed to get local addr"),
            SocketAddr::from_str(CLIENT_ADDR).unwrap()
        );

        stream
            .set_broadcast(false)
            .expect("Failed to set broadcast");
        assert!(!stream.broadcast().expect("Failed to get broadcast"));
        stream.set_broadcast(true).expect("Failed to set broadcast");
        assert!(stream.broadcast().expect("Failed to get broadcast"));

        stream
            .set_multicast_loop_v4(false)
            .expect("Failed to set multicast_loop_v4");
        assert!(!stream
            .multicast_loop_v4()
            .expect("Failed to get multicast_loop_v4"));
        stream
            .set_multicast_loop_v4(true)
            .expect("Failed to set multicast_loop_v4");
        assert!(stream
            .multicast_loop_v4()
            .expect("Failed to get multicast_loop_v4"));

        stream
            .set_multicast_ttl_v4(124)
            .expect("Failed to set multicast_ttl_v4");
        assert_eq!(
            stream
                .multicast_ttl_v4()
                .expect("Failed to get multicast_ttl_v4"),
            124
        );

        stream.set_ttl(144).expect("Failed to set ttl");
        assert_eq!(stream.ttl().expect("Failed to get ttl"), 144);

        match stream.take_error() {
            Ok(err_) => {
                if let Some(err) = err_ {
                    panic!("Take error returned with an error: {err:?}")
                }
            }
            Err(err) => panic!("Take error failed: {err:?}"),
        }

        for _ in 0..TIMES {
            stream
                .send_to_with_timeout(REQUEST, SERVER_ADDR, TIMEOUT)
                .await
                .expect("send failed");

            stream
                .poll_recv_with_timeout(TIMEOUT)
                .await
                .expect("poll failed");
            let mut buf = vec![0u8; RESPONSE.len()];

            stream
                .peek_from_with_timeout(&mut buf, TIMEOUT)
                .await
                .expect("peek failed");
            stream
                .peek_from_with_timeout(&mut buf, TIMEOUT)
                .await
                .expect("peek failed");

            stream
                .poll_recv_with_timeout(TIMEOUT)
                .await
                .expect("poll failed");
            stream
                .recv_from_with_timeout(&mut buf, TIMEOUT)
                .await
                .expect("recv failed");
        }
    }

    #[orengine::test::test_local]
    fn test_timeout() {
        const ADDR: &str = "127.0.0.1:10141";
        const TIMEOUT: Duration = Duration::from_micros(1);

        let mut socket = UdpSocket::bind(ADDR).await.expect("bind failed");

        match socket.poll_recv_with_timeout(TIMEOUT).await {
            Ok(()) => panic!("poll_recv should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }

        match socket.recv_from_with_timeout(&mut [0u8; 10], TIMEOUT).await {
            Ok(_) => panic!("recv_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}"),
        }

        match socket.peek_from_with_timeout(&mut [0u8; 10], TIMEOUT).await {
            Ok(_) => panic!("peek_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}"),
        }
    }
}

use std::fmt::{Debug, Formatter};
use std::io::Result;
use std::mem;
use std::net::SocketAddr;

use socket2::{SockAddr, SockRef};

use crate::io::recv_from::AsyncRecvFrom;
use crate::io::sys::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::io::{
    AsyncBind, AsyncClose, AsyncConnectDatagram, AsyncPeekFrom, AsyncPollFd, AsyncSendTo,
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
/// returns [`UdpConnectedSocket`](UdpConnectedSocket) which implements
/// [`ConnectedDatagram`](crate::net::connected_datagram::ConnectedDatagram),
/// [`AsyncRecv`](crate::io::AsyncRecv), [`AsyncPeek`](crate::io::AsyncPeek),
/// [`AsyncSend`](crate::io::AsyncSend).
///
/// # Examples
///
/// ## Usage without [`connect`](AsyncConnectDatagram)
///
/// ```no_run
/// use orengine::net::UdpSocket;
/// use orengine::io::{AsyncBind, AsyncPollFd, AsyncRecvFrom, AsyncSendTo};
/// use orengine::buf::full_buffer;
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
/// ```no_run
/// use orengine::buf::full_buffer;
/// use orengine::io::{AsyncBind, AsyncConnectDatagram, AsyncPollFd, AsyncRecv, AsyncSend};
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
///    connected_socket.send(&buf[..n]).await.expect("send_to failed");
/// }
/// # }
/// ```
pub struct UdpSocket {
    fd: RawFd,
}

impl Into<std::net::UdpSocket> for UdpSocket {
    fn into(self) -> std::net::UdpSocket {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::net::UdpSocket::from_raw_fd(fd) }
    }
}

impl From<std::net::UdpSocket> for UdpSocket {
    fn from(stream: std::net::UdpSocket) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl IntoRawFd for UdpSocket {
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl FromRawFd for UdpSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsFd for UdpSocket {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}

impl AsRawFd for UdpSocket {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl From<OwnedFd> for UdpSocket {
    fn from(fd: OwnedFd) -> Self {
        unsafe { Self::from_raw_fd(fd.into_raw_fd()) }
    }
}

impl Into<OwnedFd> for UdpSocket {
    fn into(self) -> OwnedFd {
        unsafe { OwnedFd::from_raw_fd(self.into_raw_fd()) }
    }
}

impl AsyncBind for UdpSocket {
    async fn new_socket(addr: &SocketAddr) -> Result<RawFd> {
        new_udp_socket(&addr).await
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

impl AsyncPollFd for UdpSocket {}

impl AsyncRecvFrom for UdpSocket {}

impl AsyncPeekFrom for UdpSocket {}

impl AsyncSendTo for UdpSocket {}

impl AsyncClose for UdpSocket {}

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

        let name = if cfg!(windows) { "socket" } else { "fd" };
        res.field(name, &self.as_raw_fd()).finish()
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
    use std::ops::Deref;
    use std::rc::Rc;
    use std::str::FromStr;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use std::{io, thread};

    use crate::io::AsyncBind;
    use crate::net::ReusePort;
    use crate::runtime::local_executor;
    use crate::sync::{LocalCondVar, LocalMutex};
    use crate::{local_yield_now, Executor};

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[orengine_macros::test]
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

        let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
        let mut is_server_ready = is_server_ready_mu.lock().unwrap();
        while *is_server_ready == false {
            is_server_ready = condvar.wait(is_server_ready).unwrap();
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

    async fn test_server_with_config(config: BindConfig) {
        const SERVER_ADDR: &str = "127.0.0.1:10082";

        let is_server_ready = Arc::new(AtomicBool::new(false));
        let is_server_ready_server_clone = is_server_ready.clone();

        thread::spawn(move || {
            Executor::init().run_with_local_future(async move {
                let mut server = UdpSocket::bind_with_config(SERVER_ADDR, &config)
                    .await
                    .expect("bind failed");

                is_server_ready_server_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                for _ in 0..TIMES {
                    server.poll_recv().await.expect("poll failed");
                    let mut buf = vec![0u8; REQUEST.len()];
                    let (n, src) = server.recv_from(&mut buf).await.expect("accept failed");
                    assert_eq!(REQUEST, &buf[..n]);

                    server.send_to(RESPONSE, &src).await.expect("send failed");
                }

                drop(server);
                local_yield_now().await;
            });
        });

        while is_server_ready.load(std::sync::atomic::Ordering::Relaxed) == false {
            thread::sleep(Duration::from_millis(1));
        }

        let stream = std::net::UdpSocket::bind("127.0.0.1:9082").expect("connect failed");
        stream.connect(SERVER_ADDR).expect("connect failed");

        for _ in 0..TIMES {
            stream.send(REQUEST).expect("send failed");

            let mut buf = vec![0u8; RESPONSE.len()];
            stream.recv(&mut buf).expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }
    }

    #[orengine_macros::test]
    fn test_server() {
        let config = BindConfig::default();
        test_server_with_config(config.reuse_port(ReusePort::Disabled)).await;
        test_server_with_config(config.reuse_port(ReusePort::Default)).await;
        test_server_with_config(config.reuse_port(ReusePort::CPU)).await;
    }

    #[orengine_macros::test]
    fn test_socket() {
        const SERVER_ADDR: &str = "127.0.0.1:10090";
        const CLIENT_ADDR: &str = "127.0.0.1:10091";
        const TIMEOUT: Duration = Duration::from_secs(3);

        let is_server_ready = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        local_executor().exec_local_future(async move {
            let mut server = UdpSocket::bind(SERVER_ADDR).await.expect("bind failed");

            {
                let (is_ready_mu, condvar) = is_server_ready.deref();
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

        let (is_server_ready_mu, condvar) = is_server_ready_server_clone.deref();
        let mut is_server_ready = is_server_ready_mu.lock().await;
        while *is_server_ready == false {
            is_server_ready = condvar.wait(is_server_ready).await;
        }

        let mut stream = UdpSocket::bind(CLIENT_ADDR).await.expect("bind failed");

        assert_eq!(
            stream.local_addr().expect("Failed to get local addr"),
            SocketAddr::from_str(CLIENT_ADDR).unwrap()
        );

        stream
            .set_broadcast(false)
            .expect("Failed to set broadcast");
        assert_eq!(stream.broadcast().expect("Failed to get broadcast"), false);
        stream.set_broadcast(true).expect("Failed to set broadcast");
        assert_eq!(stream.broadcast().expect("Failed to get broadcast"), true);

        stream
            .set_multicast_loop_v4(false)
            .expect("Failed to set multicast_loop_v4");
        assert_eq!(
            stream
                .multicast_loop_v4()
                .expect("Failed to get multicast_loop_v4"),
            false
        );
        stream
            .set_multicast_loop_v4(true)
            .expect("Failed to set multicast_loop_v4");
        assert_eq!(
            stream
                .multicast_loop_v4()
                .expect("Failed to get multicast_loop_v4"),
            true
        );

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
            Ok(err_) => match err_ {
                Some(err) => panic!("Take error returned with an error: {:?}", err),
                None => {}
            },
            Err(err) => panic!("Take error failed: {:?}", err),
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
            assert_eq!(RESPONSE, buf);
            stream
                .peek_from_with_timeout(&mut buf, TIMEOUT)
                .await
                .expect("peek failed");
            assert_eq!(RESPONSE, buf);

            stream
                .poll_recv_with_timeout(TIMEOUT)
                .await
                .expect("poll failed");
            stream
                .recv_from_with_timeout(&mut buf, TIMEOUT)
                .await
                .expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }
    }

    #[orengine_macros::test]
    fn test_timeout() {
        const ADDR: &str = "127.0.0.1:10141";
        const TIMEOUT: Duration = Duration::from_micros(1);

        let mut socket = UdpSocket::bind(ADDR).await.expect("bind failed");

        match socket.poll_recv_with_timeout(TIMEOUT).await {
            Ok(_) => panic!("poll_recv should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }

        match socket
            .recv_from_with_timeout(&mut vec![0u8; 10], TIMEOUT)
            .await
        {
            Ok(_) => panic!("recv_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }

        match socket
            .peek_from_with_timeout(&mut vec![0u8; 10], TIMEOUT)
            .await
        {
            Ok(_) => panic!("peek_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }
    }
}

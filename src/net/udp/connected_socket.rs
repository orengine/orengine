use std::fmt::{Debug, Formatter};
use std::mem;

use crate::io::sys::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::io::{AsyncClose, AsyncPeek, AsyncPollFd, AsyncRecv, AsyncSend, AsyncShutdown};
use crate::net::{ConnectedDatagram, Socket};
use crate::runtime::local_executor;

/// A UDP socket.
///
/// After creating a `UdpConnectedSocket` by
/// [`connecting`](crate::io::AsyncConnectDatagram::connect) it to a socket address,
/// data can be [sent](AsyncSend) and [received](AsyncRecv) from other socket address.
///
/// Although UDP is a connectionless protocol, this implementation provides an interface
/// to set an address where data should be sent and received from.
///
/// # Example
///
/// ```rust
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
pub struct UdpConnectedSocket {
    fd: RawFd,
}

impl From<UdpConnectedSocket> for std::net::UdpSocket {
    fn from(connected_socket: UdpConnectedSocket) -> Self {
        let fd = connected_socket.fd;
        mem::forget(connected_socket);

        unsafe { Self::from_raw_fd(fd) }
    }
}

impl From<std::net::UdpSocket> for UdpConnectedSocket {
    fn from(stream: std::net::UdpSocket) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl IntoRawFd for UdpConnectedSocket {
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl FromRawFd for UdpConnectedSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsRawFd for UdpConnectedSocket {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsFd for UdpConnectedSocket {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}

impl From<OwnedFd> for UdpConnectedSocket {
    fn from(fd: OwnedFd) -> Self {
        unsafe { Self::from_raw_fd(fd.into_raw_fd()) }
    }
}

impl From<UdpConnectedSocket> for OwnedFd {
    fn from(connected_socket: UdpConnectedSocket) -> Self {
        unsafe { Self::from_raw_fd(connected_socket.into_raw_fd()) }
    }
}

impl AsyncPollFd for UdpConnectedSocket {}

impl AsyncRecv for UdpConnectedSocket {}

impl AsyncPeek for UdpConnectedSocket {}

impl AsyncSend for UdpConnectedSocket {}

impl AsyncShutdown for UdpConnectedSocket {}

impl AsyncClose for UdpConnectedSocket {}

impl Socket for UdpConnectedSocket {}

impl ConnectedDatagram for UdpConnectedSocket {}

impl Debug for UdpConnectedSocket {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("UdpConnectedSocket");

        if let Ok(addr) = self.local_addr() {
            res.field("addr", &addr);
        }

        if let Ok(peer) = self.peer_addr() {
            res.field("peer", &peer);
        }

        let name = if cfg!(windows) { "socket" } else { "fd" };
        res.field(name, &self.as_raw_fd()).finish()
    }
}

impl Drop for UdpConnectedSocket {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_local_future(async {
            close_future
                .await
                .expect("Failed to close UDP connected socket");
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::io::{AsyncBind, AsyncConnectDatagram};
    use crate::net::udp::UdpSocket;
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use std::{io, thread};

    use super::*;
    use crate as orengine;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[orengine::test::test_local]
    fn test_connected_udp_client() {
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
            let mut is_server_ready = is_server_ready_mu.lock().unwrap();
            while !*is_server_ready {
                is_server_ready = condvar.wait(is_server_ready).unwrap();
            }
            drop(is_server_ready);
        }

        let stream = UdpSocket::bind(CLIENT_ADDR).await.expect("bind failed");
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
            connected_stream
                .send_all(REQUEST)
                .await
                .expect("send failed");
            let mut buf = vec![0u8; RESPONSE.len()];

            connected_stream.recv(&mut buf).await.expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    #[orengine::test::test_local]
    fn test_timeout() {
        const ADDR: &str = "127.0.0.1:11141";
        const TIMEOUT: Duration = Duration::from_micros(1);

        let socket = UdpSocket::bind(ADDR).await.expect("bind failed");
        let mut connected_socket = socket
            .connect_with_timeout("127.0.0.1:14142", TIMEOUT)
            .await
            .expect("bind failed");

        match connected_socket.poll_recv_with_timeout(TIMEOUT).await {
            Ok(()) => panic!("poll_recv should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }

        match connected_socket
            .recv_with_timeout(&mut [0u8; 10], TIMEOUT)
            .await
        {
            Ok(_) => panic!("recv_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }

        match connected_socket
            .peek_with_timeout(&mut [0u8; 10], TIMEOUT)
            .await
        {
            Ok(_) => panic!("peek_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }
    }
}

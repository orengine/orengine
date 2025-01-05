use crate::io::sys::{AsRawSocket, AsSocket, FromRawSocket, IntoRawSocket, RawSocket};
use crate::io::{
    AsyncPeek, AsyncPollSocket, AsyncRecv, AsyncSend, AsyncShutdown, AsyncSocketClose,
};
use crate::net::{ConnectedDatagram, Socket};
use crate::runtime::local_executor;
use std::fmt::{Debug, Formatter};
use std::mem::ManuallyDrop;

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
pub struct UdpConnectedSocket {
    raw_socket: RawSocket,
}

impl From<UdpConnectedSocket> for std::net::UdpSocket {
    fn from(connected_socket: UdpConnectedSocket) -> Self {
        unsafe { Self::from_raw_socket(ManuallyDrop::new(connected_socket).raw_socket) }
    }
}

impl From<std::net::UdpSocket> for UdpConnectedSocket {
    fn from(stream: std::net::UdpSocket) -> Self {
        Self {
            raw_socket: stream.into_raw_socket(),
        }
    }
}

#[cfg(unix)]
impl std::os::fd::IntoRawFd for UdpConnectedSocket {
    fn into_raw_fd(self) -> std::os::fd::RawFd {
        ManuallyDrop::new(self).raw_socket
    }
}

#[cfg(windows)]
impl std::os::windows::io::IntoRawSocket for UdpConnectedSocket {
    fn into_raw_socket(self) -> RawSocket {
        ManuallyDrop::new(self).raw_socket
    }
}

impl IntoRawSocket for UdpConnectedSocket {}

#[cfg(unix)]
impl std::os::fd::AsRawFd for UdpConnectedSocket {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.raw_socket
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawSocket for UdpConnectedSocket {
    fn as_raw_socket(&self) -> RawSocket {
        self.raw_socket
    }
}

impl AsRawSocket for UdpConnectedSocket {}

#[cfg(unix)]
impl std::os::fd::AsFd for UdpConnectedSocket {
    fn as_fd(&self) -> std::os::fd::BorrowedFd {
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.raw_socket) }
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for UdpConnectedSocket {
    fn as_socket(&self) -> BorrowedSocket {
        unsafe { std::os::windows::io::BorrowedSocket::borrow_raw(self.raw_socket) }
    }
}

impl AsSocket for UdpConnectedSocket {}

#[cfg(unix)]
impl std::os::fd::FromRawFd for UdpConnectedSocket {
    unsafe fn from_raw_fd(raw_fd: std::os::fd::RawFd) -> Self {
        Self { raw_socket: raw_fd }
    }
}

#[cfg(windows)]
impl std::os::windows::io::FromRawSocket for UdpConnectedSocket {
    unsafe fn from_raw_socket(raw_socket: RawSocket) -> Self {
        Self { raw_socket }
    }
}

impl FromRawSocket for UdpConnectedSocket {}

impl AsyncPollSocket for UdpConnectedSocket {}

impl AsyncRecv for UdpConnectedSocket {}

impl AsyncPeek for UdpConnectedSocket {}

impl AsyncSend for UdpConnectedSocket {}

impl AsyncShutdown for UdpConnectedSocket {}

impl AsyncSocketClose for UdpConnectedSocket {}

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

        res.field("raw_socket", &AsRawSocket::as_raw_socket(self))
            .finish()
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
    use crate::io::{get_fixed_buffer, AsyncBind, AsyncConnectDatagram};
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
                .send_all_bytes(REQUEST)
                .await
                .expect("send failed");
            let mut buf = vec![0u8; RESPONSE.len()];

            connected_stream
                .recv_bytes(&mut buf)
                .await
                .expect("recv failed");
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

        let mut buffered_request = get_fixed_buffer().await;
        buffered_request.append(REQUEST);

        match connected_socket
            .recv_with_timeout(&mut buffered_request, TIMEOUT)
            .await
        {
            Ok(_) => panic!("recv_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }

        match connected_socket
            .peek_with_timeout(&mut buffered_request, TIMEOUT)
            .await
        {
            Ok(_) => panic!("peek_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }
    }
}

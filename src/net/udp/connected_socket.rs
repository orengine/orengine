use std::mem;

use crate::io::sys::{AsRawFd, RawFd, FromRawFd, IntoRawFd, AsFd, BorrowedFd};
use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown, AsyncRecv, AsyncPeek, AsyncSend};
use crate::net::{ConnectedDatagram, Socket};
use crate::runtime::local_executor;

#[derive(Debug)]
pub struct UdpConnectedSocket {
    fd: RawFd,
}

impl Into<std::net::UdpSocket> for UdpConnectedSocket {
    fn into(self) -> std::net::UdpSocket {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::net::UdpSocket::from_raw_fd(fd) }
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

impl AsyncPollFd for UdpConnectedSocket {}

impl AsyncRecv for UdpConnectedSocket {}

impl AsyncPeek for UdpConnectedSocket {}

impl AsyncSend for UdpConnectedSocket {}

impl AsyncShutdown for UdpConnectedSocket {}

impl AsyncClose for UdpConnectedSocket {}

impl Socket for UdpConnectedSocket {}

impl ConnectedDatagram for UdpConnectedSocket {}

impl Drop for UdpConnectedSocket {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_future(async {
            close_future
                .await
                .expect("Failed to close UDP connected socket");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};
    use std::{io, thread};
    use std::time::Duration;

    use crate::io::{AsyncBind, AsyncConnectDatagram};
    use crate::net::udp::UdpSocket;

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test_macro::test]
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

        let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
        let mut is_server_ready = is_server_ready_mu.lock().unwrap();
        while *is_server_ready == false {
            is_server_ready = condvar.wait(is_server_ready).unwrap();
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

    #[test_macro::test]
    fn test_timeout() {
        const ADDR: &str = "127.0.0.1:11141";
        const TIMEOUT: Duration = Duration::from_micros(1);

        let socket = UdpSocket::bind(ADDR).await.expect("bind failed");
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
    }
}

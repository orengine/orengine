use crate::io::sys::{AsRawSocket, AsSocket, FromRawSocket, IntoRawSocket, RawSocket};
use crate::io::{
    AsyncPeek, AsyncPollSocket, AsyncRecv, AsyncSend, AsyncShutdown, AsyncSocketClose,
};
use crate::net::unix::unix_impl_socket;
use crate::net::{ConnectedDatagram, Socket};
use crate::runtime::local_executor;
use std::fmt::{Debug, Formatter};
use std::mem::ManuallyDrop;

/// A UNIX datagram.
///
/// After creating a `UnixConnectedSocket` by
/// [`connecting`](crate::io::AsyncConnectDatagram::connect) it to a socket address,
/// data can be [sent](AsyncSend) and [received](AsyncRecv) from other socket address.
///
/// Although UNIX is a connectionless protocol, this implementation provides an interface
/// to set an address where data should be sent and received from.
///
/// # Example
///
/// ```rust
/// use orengine::io::{full_buffer, AsyncBind, AsyncConnectDatagram, AsyncPollSocket, AsyncRecv, AsyncSend};
/// use orengine::net::UnixDatagram;
///
/// # async fn foo() {
/// let socket = UnixDatagram::bind("/path/to/socket").await.unwrap();
/// let mut connected_socket = socket.connect("/path/to/other/socket").await.unwrap();
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
pub struct UnixConnectedDatagram {
    raw_socket: RawSocket,
}

impl From<UnixConnectedDatagram> for std::os::unix::net::UnixDatagram {
    fn from(connected_socket: UnixConnectedDatagram) -> Self {
        unsafe { Self::from_raw_socket(ManuallyDrop::new(connected_socket).raw_socket) }
    }
}

impl From<std::os::unix::net::UnixDatagram> for UnixConnectedDatagram {
    fn from(connected_socket: std::os::unix::net::UnixDatagram) -> Self {
        Self {
            raw_socket: IntoRawSocket::into_raw_socket(connected_socket),
        }
    }
}

impl std::os::fd::IntoRawFd for UnixConnectedDatagram {
    fn into_raw_fd(self) -> std::os::fd::RawFd {
        ManuallyDrop::new(self).raw_socket
    }
}

impl IntoRawSocket for UnixConnectedDatagram {}

impl std::os::fd::AsRawFd for UnixConnectedDatagram {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.raw_socket
    }
}

impl AsRawSocket for UnixConnectedDatagram {}

impl std::os::fd::AsFd for UnixConnectedDatagram {
    fn as_fd(&self) -> std::os::fd::BorrowedFd {
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.raw_socket) }
    }
}

impl AsSocket for UnixConnectedDatagram {}

#[cfg(unix)]
impl std::os::fd::FromRawFd for UnixConnectedDatagram {
    unsafe fn from_raw_fd(raw_fd: std::os::fd::RawFd) -> Self {
        Self { raw_socket: raw_fd }
    }
}

impl FromRawSocket for UnixConnectedDatagram {}

impl AsyncPollSocket for UnixConnectedDatagram {}

impl Socket for UnixConnectedDatagram {
    unix_impl_socket!();
}

impl AsyncRecv for UnixConnectedDatagram {}

impl AsyncPeek for UnixConnectedDatagram {}

impl AsyncSend for UnixConnectedDatagram {}

impl AsyncShutdown for UnixConnectedDatagram {}

impl AsyncSocketClose for UnixConnectedDatagram {}

impl ConnectedDatagram for UnixConnectedDatagram {}

impl Debug for UnixConnectedDatagram {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("UnixConnectedSocket");

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

impl Drop for UnixConnectedDatagram {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_local_future(async {
            close_future
                .await
                .expect("Failed to close UNIX connected datagram");
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::io::{get_fixed_buffer, AsyncBind, AsyncConnectDatagram};
    use crate::net::unix::UnixDatagram;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use std::{io, thread};

    use super::*;
    use crate as orengine;
    use crate::fs;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[orengine::test::test_local]
    fn test_connected_unix_client() {
        const SERVER_ADDR: &str = "/tmp/orengine_test_connected_unix_client__server";
        const CLIENT_ADDR: &str = "/tmp/orengine_test_connected_unix_client__client";

        let _ = fs::remove_file(SERVER_ADDR).await;
        let _ = fs::remove_file(CLIENT_ADDR).await;

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            let socket =
                std::os::unix::net::UnixDatagram::bind(SERVER_ADDR).expect("std bind failed");

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

                socket
                    .send_to(RESPONSE, src.as_pathname().unwrap())
                    .expect("std write failed");
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

        let datagram = UnixDatagram::bind(CLIENT_ADDR).await.expect("bind failed");
        let mut connected_stream = datagram.connect(SERVER_ADDR).await.expect("connect failed");

        assert_eq!(
            connected_stream
                .local_addr()
                .expect(CLIENT_ADDR)
                .as_pathname()
                .unwrap()
                .to_string_lossy(),
            CLIENT_ADDR
        );
        assert_eq!(
            connected_stream
                .peer_addr()
                .expect(CLIENT_ADDR)
                .as_pathname()
                .unwrap()
                .to_string_lossy(),
            SERVER_ADDR
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
        const ADDR: &str = "/tmp/orengine_test_unix_datagram_timeout";
        const ANOTHER_ADDR: &str = "/tmp/orengine_test_timeout_unix_datagram_another";
        const TIMEOUT: Duration = Duration::from_micros(1);

        let _ = fs::remove_file(ADDR).await;
        let _ = fs::remove_file(ANOTHER_ADDR).await;

        let socket = UnixDatagram::bind(ADDR).await.expect("bind failed");
        let another_socket = UnixDatagram::bind(ANOTHER_ADDR).await.expect("bind failed");
        let mut connected_socket = socket
            .connect_with_timeout(ANOTHER_ADDR, TIMEOUT)
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

        drop(another_socket);
    }
}

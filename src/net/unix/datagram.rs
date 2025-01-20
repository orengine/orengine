use socket2::SockRef;
use std::fmt::{Debug, Formatter};
use std::io::Result;
use std::mem::ManuallyDrop;

use crate::io::sys::{
    AsRawSocket, AsSocket, BorrowedSocket, FromRawSocket, IntoRawSocket, RawSocket,
};
use crate::io::{
    AsyncBind, AsyncConnectDatagram, AsyncPeekFrom, AsyncPollSocket, AsyncRecvFrom, AsyncSendTo,
    AsyncSocketClose,
};
use crate::net::addr::{IntoSockAddr, ToSockAddrs};
use crate::net::creators_of_sockets::new_unix_datagram;
use crate::net::unix::connected_datagram::UnixConnectedDatagram;
use crate::net::unix::unix_impl_socket;
use crate::net::BindConfig;
use crate::net::{Datagram, Socket};
use crate::runtime::local_executor;
use crate::utils::each_addr::each_addr;

/// A Unix datagram socket
///
/// After creating a `UnixSocket` by [`bind`](UnixDatagram::bind)ing it to a socket address, data can be
/// [sent to](AsyncSendTo) and [received from](AsyncRecvFrom) any other socket address.
///
/// Although UNIX is a connectionless protocol, this implementation provides an interface
/// to set an address where data should be sent and received from.
/// [`UnixSocket::connect`](AsyncConnectDatagram)
/// returns [`UnixConnectedDatagram`] which implements
/// [`ConnectedDatagram`](crate::net::connected_datagram::ConnectedDatagram),
/// [`AsyncRecv`](crate::io::AsyncRecv), [`AsyncPeek`](crate::io::AsyncPeek),
/// [`AsyncSend`](crate::io::AsyncSend).
///
/// # OS Support
///
/// This structure is only supported on Unix platforms and is not available on Windows.
///
/// # Examples
///
/// ## Usage without [`connect`](AsyncConnectDatagram)
///
/// ```rust
/// use orengine::net::UnixDatagram;
/// use orengine::io::{full_buffer, AsyncBind, AsyncPollSocket, AsyncRecvFrom, AsyncSendTo};
///
/// # async fn foo() {
/// let mut socket = UnixDatagram::bind("/tmp/sock").await.unwrap();
/// loop {
///    socket.poll_recv().await.expect("poll failed");
///    let mut buf = full_buffer();
///    let (n, addr) = socket.recv_bytes_from(&mut buf).await.expect("recv_from failed");
///    if n == 0 {
///        continue;
///    }
///
///    socket.send_bytes_to(&buf[..n], addr.as_pathname().unwrap()).await.expect("send_to failed");
/// }
/// # }
/// ```
///
/// ## Usage with [`connect`](AsyncConnectDatagram)
///
/// ```rust
/// use orengine::io::{full_buffer, AsyncBind, AsyncConnectDatagram, AsyncPollSocket, AsyncRecv, AsyncSend};
/// use orengine::net::UnixDatagram;
///
/// # async fn foo() {
/// let socket = UnixDatagram::bind("/tmp/sock").await.unwrap();
/// let mut connected_socket = socket.connect("/tmp/other_sock").await.unwrap();
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
pub struct UnixDatagram {
    raw_socket: RawSocket,
}

impl From<UnixDatagram> for std::os::unix::net::UnixDatagram {
    fn from(socket: UnixDatagram) -> Self {
        unsafe { Self::from_raw_socket(ManuallyDrop::new(socket).raw_socket) }
    }
}

impl From<std::os::unix::net::UnixDatagram> for UnixDatagram {
    fn from(socket: std::os::unix::net::UnixDatagram) -> Self {
        Self {
            raw_socket: IntoRawSocket::into_raw_socket(socket),
        }
    }
}

impl std::os::fd::IntoRawFd for UnixDatagram {
    fn into_raw_fd(self) -> std::os::fd::RawFd {
        ManuallyDrop::new(self).raw_socket
    }
}

impl IntoRawSocket for UnixDatagram {}

impl std::os::fd::AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.raw_socket
    }
}

impl AsRawSocket for UnixDatagram {}

impl std::os::fd::AsFd for UnixDatagram {
    fn as_fd(&self) -> std::os::fd::BorrowedFd {
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.raw_socket) }
    }
}

impl AsSocket for UnixDatagram {}

impl std::os::fd::FromRawFd for UnixDatagram {
    unsafe fn from_raw_fd(raw_fd: std::os::fd::RawFd) -> Self {
        Self { raw_socket: raw_fd }
    }
}

impl AsyncPollSocket for UnixDatagram {}

impl Socket for UnixDatagram {
    unix_impl_socket!();
}

impl FromRawSocket for UnixDatagram {}

impl AsyncBind for UnixDatagram {
    async fn new_socket(_: &Self::Addr) -> Result<RawSocket> {
        new_unix_datagram().await
    }

    fn bind_and_listen_if_needed(
        sock_ref: SockRef<'_>,
        addr: Self::Addr,
        _config: &BindConfig,
    ) -> Result<()> {
        sock_ref.bind(&addr.into_sock_addr())
    }

    #[allow(
        clippy::future_not_send,
        reason = "It is not send when the addrs are not, it is fine."
    )]
    async fn bind_with_config<A: ToSockAddrs<Self::Addr>>(
        addrs: A,
        _config: &BindConfig,
    ) -> Result<Self> {
        each_addr(addrs, |addr| async move {
            let raw_socket = Self::new_socket(&addr).await?;
            let borrowed_socket = unsafe { BorrowedSocket::borrow_raw(raw_socket) };
            let sock_ref = SockRef::from(&borrowed_socket);

            sock_ref.bind(&addr.into_sock_addr())?;

            Ok(Self { raw_socket })
        })
        .await
    }
}

impl AsyncConnectDatagram<UnixConnectedDatagram> for UnixDatagram {}

impl AsyncRecvFrom for UnixDatagram {}

impl AsyncPeekFrom for UnixDatagram {}

impl AsyncSendTo for UnixDatagram {}

impl AsyncSocketClose for UnixDatagram {}

impl Datagram for UnixDatagram {
    type ConnectedDatagram = UnixConnectedDatagram;
}

impl Debug for UnixDatagram {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("UnixSocket");

        if let Ok(addr) = self.local_addr() {
            res.field("local addr", &addr);
        }

        res.field("raw_socket", &AsRawSocket::as_raw_socket(self))
            .finish()
    }
}

impl Drop for UnixDatagram {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_local_future(async {
            close_future.await.expect("Failed to close UNIX datagram");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::io::{AsyncBind, AsyncRecv, AsyncSend};
    use crate::runtime::local_executor;
    use crate::sync::{AsyncCondVar, AsyncMutex, LocalCondVar, LocalMutex};
    use crate::{fs, yield_now};
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use std::{io, thread};

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[orengine::test::test_local]
    fn test_unix_datagram_client() {
        const SERVER_ADDR: &str = "/tmp/orengine_test_unix_datagram_client__server";
        const CLIENT_ADDR: &str = "/tmp/orengine_unix_datagram_unix_client__client";

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

            loop {
                let is_server_ready = is_server_ready_mu.lock().expect("lock failed");
                if *is_server_ready {
                    drop(is_server_ready);
                    break;
                }

                let _unused = condvar.wait(is_server_ready).expect("wait failed");
            }
        }

        let mut datagram = UnixDatagram::bind(CLIENT_ADDR).await.expect("bind failed");

        for _ in 0..TIMES {
            datagram
                .send_bytes_to(REQUEST, SERVER_ADDR)
                .await
                .expect("send failed");
            let mut buf = vec![0u8; RESPONSE.len()];

            datagram
                .recv_bytes_from(&mut buf)
                .await
                .expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    #[orengine::test::test_local]
    fn test_unix_datagram_server() {
        const SERVER_ADDR: &str = "/tmp/orengine_unix_datagram_unix_server__server";
        const CLIENT_ADDR: &str = "/tmp/orengine_unix_datagram_unix_server__client";

        let _ = fs::remove_file(SERVER_ADDR).await;
        let _ = fs::remove_file(CLIENT_ADDR).await;

        let is_server_ready = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
        let is_server_ready_clone = is_server_ready.clone();

        local_executor().spawn_local(async move {
            let mut server = UnixDatagram::bind(SERVER_ADDR).await.expect("bind failed");

            *is_server_ready_clone.0.lock().await = true;
            is_server_ready_clone.1.notify_one();

            for _ in 0..TIMES {
                server
                    .poll_recv_with_timeout(Duration::from_secs(10))
                    .await
                    .expect("poll failed");
                let mut buf = vec![0u8; REQUEST.len()];
                let (n, src) = server
                    .recv_bytes_from_with_timeout(&mut buf, Duration::from_secs(10))
                    .await
                    .expect("accept failed");
                assert_eq!(REQUEST, &buf[..n]);

                server
                    .send_bytes_to_with_timeout(
                        RESPONSE,
                        src.as_pathname().unwrap(),
                        Duration::from_secs(10),
                    )
                    .await
                    .expect("send failed");
            }
        });

        let mut is_server_ready_guard = is_server_ready.0.lock().await;
        while !*is_server_ready_guard {
            is_server_ready_guard = is_server_ready.1.wait(is_server_ready_guard).await;
        }

        let mut socket = UnixDatagram::bind(CLIENT_ADDR)
            .await
            .expect("connect failed")
            .connect_with_timeout(SERVER_ADDR, Duration::from_secs(10))
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
    fn test_unix_datagram_socket() {
        const SERVER_ADDR: &str = "/tmp/orengine_test_unix_datagram_socket__server";
        const CLIENT_ADDR: &str = "/tmp/orengine_test_unix_datagram_socket__client";
        const TIMEOUT: Duration = Duration::from_secs(3);

        let _ = fs::remove_file(SERVER_ADDR).await;
        let _ = fs::remove_file(CLIENT_ADDR).await;

        let is_server_ready = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        local_executor().exec_local_future(async move {
            let mut server = UnixDatagram::bind(SERVER_ADDR).await.expect("bind failed");

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
                    .recv_bytes_from_with_timeout(&mut buf, TIMEOUT)
                    .await
                    .expect("accept failed");
                assert_eq!(REQUEST, &buf[..n]);
                assert_eq!(src.as_pathname(), Some(CLIENT_ADDR.as_ref()));

                server
                    .send_bytes_to_with_timeout(RESPONSE, src.as_pathname().unwrap(), TIMEOUT)
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

        let mut datagram = UnixDatagram::bind(CLIENT_ADDR).await.expect("bind failed");

        assert_eq!(
            datagram
                .local_addr()
                .expect("Failed to get local addr")
                .as_pathname()
                .unwrap()
                .to_string_lossy(),
            CLIENT_ADDR
        );

        match datagram.take_error() {
            Ok(err_) => {
                if let Some(err) = err_ {
                    panic!("Take error returned with an error: {err:?}")
                }
            }
            Err(err) => panic!("Take error failed: {err:?}"),
        }

        for _ in 0..TIMES {
            datagram
                .send_bytes_to_with_timeout(REQUEST, SERVER_ADDR, TIMEOUT)
                .await
                .expect("send failed");

            datagram
                .poll_recv_with_timeout(TIMEOUT)
                .await
                .expect("poll failed");
            let mut buf = vec![0u8; RESPONSE.len()];

            datagram
                .peek_bytes_from_with_timeout(&mut buf, TIMEOUT)
                .await
                .expect("peek failed");
            datagram
                .peek_bytes_from_with_timeout(&mut buf, TIMEOUT)
                .await
                .expect("peek failed");

            datagram
                .poll_recv_with_timeout(TIMEOUT)
                .await
                .expect("poll failed");
            datagram
                .recv_bytes_from_with_timeout(&mut buf, TIMEOUT)
                .await
                .expect("recv failed");
        }
    }

    #[orengine::test::test_local]
    fn test_unix_datagram_timeout() {
        const ADDR: &str = "/tmp/orengine_test_unix_connected_datagram_timeout";
        const TIMEOUT: Duration = Duration::from_micros(1);

        let _ = fs::remove_file(ADDR).await;

        let mut socket = UnixDatagram::bind(ADDR).await.expect("bind failed");

        match socket.poll_recv_with_timeout(TIMEOUT).await {
            Ok(()) => panic!("poll_recv should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut),
        }

        match socket
            .recv_bytes_from_with_timeout(&mut [0u8; 10], TIMEOUT)
            .await
        {
            Ok(_) => panic!("recv_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}"),
        }

        match socket
            .peek_bytes_from_with_timeout(&mut [0u8; 10], TIMEOUT)
            .await
        {
            Ok(_) => panic!("peek_from should timeout"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}"),
        }
    }
}

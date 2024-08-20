use std::io::{Result};
use std::net::ToSocketAddrs;
use std::mem;
use std::os::fd::{AsFd, BorrowedFd};

use socket2::SockAddr;

use crate::io::bind::BindConfig;
use crate::io::recv_from::AsyncRecvFrom;
use crate::io::sys::{AsRawFd, RawFd, IntoRawFd, FromRawFd};
use crate::io::{AsyncClose, AsyncPollFd, AsyncBind, AsyncConnectDatagram, AsyncPeekFrom, AsyncSendTo};
use crate::net::creators_of_sockets::new_udp_socket;
use crate::net::udp::connected_socket::UdpConnectedSocket;
use crate::{each_addr, Executor};
use crate::net::{Datagram, Socket};

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

impl AsyncBind for UdpSocket {
    async fn bind_with_config<A: ToSocketAddrs>(addrs: A, config: &BindConfig) -> Result<Self> {
        each_addr!(&addrs, async move |addr| {
            let fd = new_udp_socket(&addr).await?;
            let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
            let socket_ref = socket2::SockRef::from(&borrowed_fd);

            if config.only_v6 {
                socket_ref.set_only_v6(true)?;
            }

            if config.reuse_address {
                socket_ref.set_reuse_address(true)?;
            }

            if config.reuse_port {
                socket_ref.set_reuse_port(true)?;
            }

            socket_ref.bind(&SockAddr::from(addr))?;

            Ok(Self {
                fd
            })
        })
    }
}

impl AsyncConnectDatagram<UdpConnectedSocket> for UdpSocket {}

impl AsyncPollFd for UdpSocket {}

impl AsyncRecvFrom for UdpSocket {}

impl AsyncPeekFrom for UdpSocket {}

impl AsyncSendTo for UdpSocket {}

impl AsyncClose for UdpSocket {}

impl Socket for UdpSocket {}

impl Datagram<UdpConnectedSocket> for UdpSocket {}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        let close_future = self.close();
        Executor::exec_future(async {
            close_future.await.expect("Failed to close UDP socket");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::{io, thread};
    use std::time::Duration;
    use crate::Executor;

    use crate::io::AsyncBind;
    use crate::runtime::create_local_executer_for_block_on;
    use crate::sync::cond_var::LocalCondVar;
    use crate::sync::LocalMutex;

    use super::*;

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

            let mut stream = UdpSocket::bind("127.0.0.1:9081").await.expect("bind failed");

            for _ in 0..TIMES {
                stream
                    .send_to(REQUEST, SERVER_ADDR)
                    .await
                    .expect("send failed");
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
                let mut server = UdpSocket::bind(SERVER_ADDR).await.expect("bind failed");

                is_server_ready_server_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                for _ in 0..TIMES {
                    server.poll_recv().await.expect("poll failed");
                    let mut buf = vec![0u8; REQUEST.len()];
                    let (n, src) = server.recv_from(&mut buf).await.expect("accept failed");
                    assert_eq!(REQUEST, &buf[..n]);

                    server.send_to(RESPONSE, &src).await.expect("send failed");
                }
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

        server_thread.join().expect("server thread join failed");
    }

    #[test]
    fn test_socket() {
        const SERVER_ADDR: &str = "127.0.0.1:10090";
        const CLIENT_ADDR: &str = "127.0.0.1:10091";
        const TIMEOUT: Duration = Duration::from_secs(3);

        let is_server_ready = (LocalMutex::new(false), LocalCondVar::new());
        let is_server_ready_server_clone = is_server_ready.clone();

        create_local_executer_for_block_on(async move {
            Executor::exec_future(async {
                let mut server = UdpSocket::bind(SERVER_ADDR).await.expect("bind failed");

                {
                    let (is_ready_mu, condvar) = is_server_ready;
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

            let (is_server_ready_mu, condvar) = is_server_ready_server_clone;
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
                    Some(err) => panic!("Take error returned with an error: {err:?}"),
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
        });
    }

    #[test]
    fn test_timeout() {
        const ADDR: &str = "127.0.0.1:10141";
        const TIMEOUT: Duration = Duration::from_micros(1);

        create_local_executer_for_block_on(async {
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
        });
    }
}

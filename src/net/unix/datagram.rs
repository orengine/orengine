use std::{io, mem};
use std::os::fd::{AsFd, BorrowedFd};

use socket2::{Domain, SockAddr, Socket, SockRef, Type};

use crate::io::sys::{AsRawFd, RawFd, FromRawFd, IntoRawFd};
use crate::io::{
    AsyncBindUnix,
    AsyncClose,
    AsyncConnectDatagramUnix,
    AsyncPeekFromUnix,
    AsyncPollFd,
    AsyncRecvFromUnix,
    AsyncSendToUnix,
    AsyncShutdown,
    AsPath
};
use crate::net::unix::connected_socket_datagram::UnixConnectedDatagram;
use crate::runtime::local_executor;
use crate::net::unix::path_datagram::PathDatagram;
use crate::net::unix::path_socket::PathSocket;

pub struct UnixDatagram {
    fd: RawFd,
}

impl Into<std::os::unix::net::UnixDatagram> for UnixDatagram {
    #[inline(always)]
    fn into(self) -> std::os::unix::net::UnixDatagram {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::os::unix::net::UnixDatagram::from_raw_fd(fd) }
    }
}

impl IntoRawFd for UnixDatagram {
    #[inline(always)]
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl From<std::os::unix::net::UnixDatagram> for UnixDatagram {
    #[inline(always)]
    fn from(stream: std::os::unix::net::UnixDatagram) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl FromRawFd for UnixDatagram {
    #[inline(always)]
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsRawFd for UnixDatagram {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsyncPollFd for UnixDatagram {}

impl AsyncShutdown for UnixDatagram {}

impl AsyncClose for UnixDatagram {}

impl AsFd for UnixDatagram {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) }
    }
}

impl AsyncBindUnix for UnixDatagram {
    async fn bind_with_backlog_size<P: AsPath>(path: P, _backlog_size: isize) -> io::Result<Self> {
        let s = Self::unbound().await?;

        let fd = s.as_fd();
        let socket_ref = SockRef::from(&fd);

        socket_ref.bind(&SockAddr::unix(path)?)?;

        Ok(s)
    }
}

impl AsyncConnectDatagramUnix<UnixConnectedDatagram> for UnixDatagram {}

impl AsyncSendToUnix for UnixDatagram {}

impl AsyncRecvFromUnix for UnixDatagram {}

impl AsyncPeekFromUnix for UnixDatagram {}

impl PathSocket for UnixDatagram {}

impl PathDatagram<UnixConnectedDatagram> for UnixDatagram {
    #[inline(always)]
    async fn pair() -> io::Result<(Self, Self)> {
        let (socket1, socket2) = Socket::pair(Domain::UNIX, Type::DGRAM, None)?;
        Ok((
            UnixDatagram {
                fd: socket1.into_raw_fd(),
            },
            UnixDatagram {
                fd: socket2.into_raw_fd(),
            },
        ))
    }
}

impl Drop for UnixDatagram {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future
                .await
                .expect("Failed to close UNIX datagram socket");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use crate::fs::test_helper::delete_file_if_exists;
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
        const SERVER_ADDR: &str = "/tmp/test_socket_datagram_client_server.sock";
        const CLIENT_ADDR: &str = "/tmp/test_socket_datagram_client_client.sock";

        delete_file_if_exists(SERVER_ADDR);
        delete_file_if_exists(CLIENT_ADDR);

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            let socket =
                std::os::unix::net::UnixDatagram::bind(SERVER_ADDR).expect("std bind failed");
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

                println!("TODO r it. src: {:?}", src);

                socket
                    .send_to_addr(RESPONSE, &src)
                    .expect("std write failed");
            }
        });

        create_local_executer_for_block_on(async move {
            let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
            let mut is_server_ready = is_server_ready_mu.lock().unwrap();
            while *is_server_ready == false {
                is_server_ready = condvar.wait(is_server_ready).unwrap();
            }

            let mut stream = UnixDatagram::bind(CLIENT_ADDR).await.expect("bind failed");

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
        const SERVER_ADDR: &str = "/tmp/test_socket_datagram_server_server.sock";
        const CLIENT_ADDR: &str = "/tmp/test_socket_datagram_server_client.sock";

        delete_file_if_exists(SERVER_ADDR);
        delete_file_if_exists(CLIENT_ADDR);

        let is_server_ready = Arc::new(AtomicBool::new(false));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            create_local_executer_for_block_on(async move {
                let mut server = UnixDatagram::bind(SERVER_ADDR).await.expect("bind failed");

                is_server_ready_server_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                for _ in 0..TIMES {
                    server.poll_recv().await.expect("poll failed");
                    let mut buf = vec![0u8; REQUEST.len()];
                    let (n, src) = server.recv_from(&mut buf).await.expect("accept failed");
                    assert_eq!(REQUEST, &buf[..n]);

                    server.send_to(RESPONSE, &src.as_pathname().unwrap()).await.expect("send failed");
                }
            });
        });

        while is_server_ready.load(std::sync::atomic::Ordering::Relaxed) == false {
            thread::sleep(Duration::from_millis(1));
        }

        let stream = std::os::unix::net::UnixDatagram::bind(CLIENT_ADDR).expect("connect failed");
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
        const SERVER_ADDR: &str = "/tmp/test_socket_datagram_socket_server.sock";
        const CLIENT_ADDR: &str = "/tmp/test_socket_datagram_socket_client.sock";
        const TIMEOUT: Duration = Duration::from_secs(3);

        delete_file_if_exists(SERVER_ADDR);
        delete_file_if_exists(CLIENT_ADDR);

        let is_server_ready = (LocalMutex::new(false), LocalCondVar::new());
        let is_server_ready_server_clone = is_server_ready.clone();

        create_local_executer_for_block_on(async move {
            local_executor().exec_future(async {
                let mut server = UnixDatagram::bind(SERVER_ADDR).await.expect("bind failed");

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
                        .send_to_with_timeout(RESPONSE, &src.as_pathname().unwrap(), TIMEOUT)
                        .await
                        .expect("send failed");
                }
            });

            let (is_server_ready_mu, condvar) = is_server_ready_server_clone;
            let mut is_server_ready = is_server_ready_mu.lock().await;
            while *is_server_ready == false {
                is_server_ready = condvar.wait(is_server_ready).await;
            }

            let mut stream = UnixDatagram::bind(CLIENT_ADDR).await.expect("bind failed");

            assert_eq!(
                stream
                    .local_addr()
                    .expect("Failed to get local addr")
                    .as_pathname()
                    .expect("Failed to get local pathname"),
                Path::new(CLIENT_ADDR)
            );

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
        const SERVER_ADDR: &str = "/tmp/test_socket_datagram_with_timeout_server.sock";
        const TIMEOUT: Duration = Duration::from_secs(3);

        delete_file_if_exists(SERVER_ADDR);

        create_local_executer_for_block_on(async {
            let mut socket = UnixDatagram::bind(SERVER_ADDR).await.expect("bind failed");

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

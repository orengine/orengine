use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::path::Path;
use std::time::Duration;
use std::{io, mem};

use socket2::{Domain, SockAddr, Socket, Type};

use crate::io::recv_from::AsyncRecvFromPath;
use crate::io::sys::{AsFd, Fd};
use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown, Connect};
use crate::net::creators_of_sockets::new_unix_socket_datagram;
use crate::net::unix::connected_socket_datagram::ConnectedSocketDatagram;
use crate::runtime::local_executor;
use crate::{each_addr, generate_peek_from_unix, generate_send_all_to_unix, generate_send_to_unix};

// TODO docs
pub struct SocketDatagram {
    fd: Fd,
}

impl SocketDatagram {
    // TODO macro
    #[inline(always)]
    pub fn fd(&mut self) -> Fd {
        self.fd
    }

    #[inline(always)]
    #[cfg(unix)]
    pub fn borrow_fd(&self) -> BorrowedFd {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }

    // TODO macro

    #[inline(always)]
    fn bind_with_backlog_size_and_sock_addr(
        addr: &SockAddr,
        backlog_size: i32,
    ) -> io::Result<Self> {
        println!("addr: {:?}, backlog_size: {}", addr, backlog_size);
        let socket = new_unix_socket_datagram()?;

        socket.bind(addr)?;

        Ok(Self {
            fd: socket.into_raw_fd(),
        })
    }

    #[inline(always)]
    pub fn bind_with_backlog_size<P: AsRef<Path>>(path: P, backlog_size: i32) -> io::Result<Self> {
        let addr = SockAddr::unix(path.as_ref())?;
        Self::bind_with_backlog_size_and_sock_addr(&addr, backlog_size)
    }

    #[inline(always)]
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::bind_with_backlog_size(path, 1)
    }

    #[inline(always)]
    pub fn bind_addr_with_backlog_size<A: ToSocketAddrs>(
        addr: A,
        backlog_size: i32,
    ) -> io::Result<Self> {
        let addr = SockAddr::from(
            addr.to_socket_addrs()?
                .next()
                .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?,
        );
        Self::bind_with_backlog_size_and_sock_addr(&addr, backlog_size)
    }

    #[inline(always)]
    pub fn bind_addr<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        Self::bind_addr_with_backlog_size(addr, 1)
    }

    #[inline(always)]
    pub fn unbound() -> io::Result<Self> {
        let socket = new_unix_socket_datagram()?;
        Ok(Self {
            fd: socket.into_raw_fd(),
        })
    }

    #[inline(always)]
    pub fn pair() -> io::Result<(Self, Self)> {
        let (socket1, socket2) = Socket::pair(Domain::UNIX, Type::DGRAM, None)?;
        Ok((
            SocketDatagram {
                fd: socket1.into_raw_fd(),
            },
            SocketDatagram {
                fd: socket2.into_raw_fd(),
            },
        ))
    }

    #[inline(always)]
    pub async fn connect<P: AsRef<Path>>(self, path: P) -> io::Result<ConnectedSocketDatagram> {
        let fd = self.fd;

        let res = Connect::new(fd, SockAddr::unix(path)?).await;

        match res {
            Ok(connected_socket) => {
                mem::forget(self);
                Ok(connected_socket)
            }
            Err(e) => Err(e),
        }
    }

    #[inline(always)]
    pub async fn connect_addr<A: ToSocketAddrs>(
        self,
        addrs: A,
    ) -> io::Result<ConnectedSocketDatagram> {
        let fd = self.fd;

        let res = each_addr!(&addrs, async move |addr: SocketAddr| -> io::Result<
            ConnectedSocketDatagram,
        > {
            Connect::new(fd, SockAddr::from(addr)).await
        });

        match res {
            Ok(connected_socket) => {
                mem::forget(self);
                Ok(connected_socket)
            }
            Err(e) => Err(e),
        }
    }

    generate_send_to_unix!();

    generate_send_all_to_unix!();

    generate_peek_from_unix!();

    #[inline(always)]
    pub fn local_addr(&self) -> io::Result<std::os::unix::net::SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_unix().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    #[inline(always)]
    pub fn peer_addr(&self) -> io::Result<std::os::unix::net::SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.peer_addr()?.as_unix().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    // TODO macro for it
    #[inline(always)]
    pub fn set_mark(&self, mark: u32) -> io::Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_mark(mark)
    }

    #[inline(always)]
    pub fn mark(&self) -> io::Result<u32> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.mark()
    }

    #[inline(always)]
    pub fn take_error(&self) -> io::Result<Option<Error>> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.take_error()
    }
}

impl Into<std::os::unix::net::UnixDatagram> for SocketDatagram {
    #[inline(always)]
    fn into(self) -> std::os::unix::net::UnixDatagram {
        unsafe { std::os::unix::net::UnixDatagram::from_raw_fd(self.fd) }
    }
}

impl From<std::os::unix::net::UnixDatagram> for SocketDatagram {
    #[inline(always)]
    fn from(stream: std::os::unix::net::UnixDatagram) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl From<Fd> for SocketDatagram {
    fn from(fd: Fd) -> Self {
        Self { fd }
    }
}

impl AsFd for SocketDatagram {
    #[inline(always)]
    fn as_raw_fd(&self) -> Fd {
        self.fd
    }
}

impl AsyncPollFd for SocketDatagram {}

impl AsyncRecvFromPath for SocketDatagram {}

impl AsyncShutdown for SocketDatagram {}

impl AsyncClose for SocketDatagram {}

impl Drop for SocketDatagram {
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
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use crate::fs::test_helper::delete_file_if_exists;
    use crate::io::Bind;
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

            let mut stream = SocketDatagram::bind(CLIENT_ADDR).expect("bind failed");

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
                let mut server = SocketDatagram::bind(SERVER_ADDR).expect("bind failed");

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
                let mut server = SocketDatagram::bind(SERVER_ADDR).expect("bind failed");

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

            let mut stream = SocketDatagram::bind(CLIENT_ADDR).expect("bind failed");

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
            let mut socket = SocketDatagram::bind(SERVER_ADDR).expect("bind failed");

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

use std::mem;

use crate::io::sys::{AsRawFd, BorrowedFd, IntoRawFd, FromRawFd, RawFd, AsFd};
use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown, AsyncRecv, AsyncPeek, AsyncSend};
use crate::net::unix::path_connected_datagram::PathConnectedDatagram;
use crate::net::unix::path_socket::PathSocket;
use crate::runtime::local_executor;

#[derive(Debug)]
pub struct UnixConnectedDatagram {
    fd: RawFd,
}

impl Into<std::os::unix::net::UnixDatagram> for UnixConnectedDatagram {
    fn into(self) -> std::os::unix::net::UnixDatagram {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::os::unix::net::UnixDatagram::from_raw_fd(fd) }
    }
}

impl From<std::os::unix::net::UnixDatagram> for UnixConnectedDatagram {
    fn from(stream: std::os::unix::net::UnixDatagram) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl IntoRawFd for UnixConnectedDatagram {
    #[inline(always)]
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl FromRawFd for UnixConnectedDatagram {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsRawFd for UnixConnectedDatagram {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd  {
        self.fd
    }
}

impl AsFd for UnixConnectedDatagram {
    #[inline(always)]
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}

impl AsyncPollFd for UnixConnectedDatagram {}

impl AsyncSend for UnixConnectedDatagram {}

impl AsyncRecv for UnixConnectedDatagram {}

impl AsyncPeek for UnixConnectedDatagram {}

impl AsyncShutdown for UnixConnectedDatagram {}

impl AsyncClose for UnixConnectedDatagram {}

impl PathSocket for UnixConnectedDatagram {}

impl PathConnectedDatagram for UnixConnectedDatagram {}

impl Drop for UnixConnectedDatagram {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future
                .await
                .expect("Failed to close UDP connected socket");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use crate::fs::test_helper::delete_file_if_exists;
    use crate::io::{AsyncBind, AsyncBindUnix, AsyncConnectDatagramUnix};
    use crate::net::unix::datagram::UnixDatagram;
    use crate::runtime::create_local_executer_for_block_on;

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test]
    fn test_client() {
        const SERVER_ADDR: &str = "/tmp/test_connected_socket_datagram_server.sock";
        const CLIENT_ADDR: &str = "/tmp/test_connected_socket_datagram_client.sock";

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

            let stream = UnixDatagram::bind(CLIENT_ADDR).await.expect("bind failed");
            let mut connected_stream = stream.connect(SERVER_ADDR).await.expect("connect failed");

            assert_eq!(
                connected_stream
                    .local_addr()
                    .expect(CLIENT_ADDR)
                    .as_pathname()
                    .unwrap(),
                Path::new(CLIENT_ADDR)
            );
            assert_eq!(
                connected_stream
                    .peer_addr()
                    .expect(CLIENT_ADDR)
                    .as_pathname()
                    .unwrap(),
                Path::new(SERVER_ADDR)
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
        });

        server_thread.join().expect("server thread join failed");
    }
}

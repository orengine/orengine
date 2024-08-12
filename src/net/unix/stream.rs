use std::{io, mem};

use socket2::{Domain, Socket, Type};

use crate::io::sys::{AsRawFd, BorrowedFd, IntoRawFd, AsFd, RawFd, FromRawFd};
use crate::io::{AsyncClose, AsyncConnectStreamUnix, AsyncPeek, AsyncPollFd, AsyncRecv, AsyncSend, AsyncShutdown};
use crate::net::unix::path_socket::PathSocket;
use crate::runtime::local_executor;
use crate::net::unix::path_stream::PathStream;

// TODO update docs
pub struct UnixStream {
    fd: RawFd,
}

impl AsRawFd for UnixStream {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Into<std::os::unix::net::UnixStream> for UnixStream {
    fn into(self) -> std::os::unix::net::UnixStream {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) }
    }
}

impl From<std::os::unix::net::UnixStream> for UnixStream {
    fn from(stream: std::os::unix::net::UnixStream) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl IntoRawFd for UnixStream {
    fn into_raw_fd(self) -> RawFd {
        todo!()
    }
}

impl AsFd for UnixStream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        todo!()
    }
}

impl AsyncConnectStreamUnix for UnixStream {
    fn unbound() -> io::Result<Self> {
        todo!()
    }
}

impl AsyncPollFd for UnixStream {}

impl AsyncSend for UnixStream {}

impl AsyncRecv for UnixStream {}

impl AsyncPeek for UnixStream {}

impl AsyncShutdown for UnixStream {}

impl AsyncClose for UnixStream {}

impl PathSocket for UnixStream {}

impl PathStream for UnixStream {
    #[inline(always)]
    async fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let (socket1, socket2) = Socket::pair(Domain::UNIX, Type::STREAM, None)?;
        Ok((
            UnixStream {
                fd: socket1.into_raw_fd(),
            },
            UnixStream {
                fd: socket2.into_raw_fd(),
            },
        ))
    }
}

impl Drop for UnixStream {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future.await.expect("Failed to close TCP stream");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration};
    use crate::fs::test_helper::{delete_file_if_exists};

    use crate::io::{AsyncAcceptUnix, AsyncBind, AsyncBindUnix};
    use crate::net::unix::UnixListener;
    use crate::runtime::create_local_executer_for_block_on;
    use crate::sync::{LocalWaitGroup};

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test]
    fn test_client() {
        const ADDR: &str = "/tmp/test_client.sock";

        delete_file_if_exists(ADDR);

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            use std::io::{Read, Write};
            let listener = std::os::unix::net::UnixListener::bind(ADDR).expect("std bind failed");

            {
                let (is_ready_mu, condvar) = &*is_server_ready;
                let mut is_ready = is_ready_mu.lock().unwrap();
                *is_ready = true;
                condvar.notify_one();
            }

            let mut stream = listener.accept().expect("accept failed").0;

            for _ in 0..TIMES {
                let mut buf = vec![0u8; REQUEST.len()];
                stream.read_exact(&mut buf).expect("std read failed");
                assert_eq!(REQUEST, buf);

                stream.write_all(RESPONSE).expect("std write failed");
            }
        });

        create_local_executer_for_block_on(async move {
            let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
            let mut is_server_ready = is_server_ready_mu.lock().unwrap();
            while *is_server_ready == false {
                is_server_ready = condvar.wait(is_server_ready).unwrap();
            }

            let mut stream = UnixStream::connect(ADDR).await.expect("connect failed");

            for _ in 0..TIMES {
                stream.send_all(REQUEST).await.expect("send failed");

                stream.poll_recv().await.expect("poll failed");
                let mut buf = vec![0u8; RESPONSE.len()];
                stream.recv_exact(&mut buf).await.expect("recv failed");
                assert_eq!(RESPONSE, buf);
            }
        });

        server_thread.join().expect("server thread join failed");
    }

    #[test]
    fn test_server() {
        const ADDR: &str = "/tmp/test_server.sock";

        delete_file_if_exists(ADDR);

        let is_server_ready = Arc::new(AtomicBool::new(false));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            create_local_executer_for_block_on(async move {
                let mut listener = UnixListener::bind(ADDR).await.expect("bind failed");

                is_server_ready_server_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                let (mut stream, _) = listener.accept().await.expect("accept failed");

                for _ in 0..TIMES {
                    stream.poll_recv().await.expect("poll failed");
                    let mut buf = vec![0u8; REQUEST.len()];
                    stream.recv_exact(&mut buf).await.expect("recv failed");
                    assert_eq!(REQUEST, buf);

                    stream.send_all(RESPONSE).await.expect("send failed");
                }
            });
        });

        use std::io::{Read, Write};
        while is_server_ready.load(std::sync::atomic::Ordering::Relaxed) == false {
            thread::sleep(Duration::from_millis(1));
        }

        let mut stream = std::os::unix::net::UnixStream::connect(ADDR).expect("connect failed");

        for _ in 0..TIMES {
            stream.write_all(REQUEST).expect("send failed");

            let mut buf = vec![0u8; RESPONSE.len()];
            stream.read_exact(&mut buf).expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    #[test]
    fn test_stream() {
        const ADDR: &str = "/tmp/test_stream.sock";

        delete_file_if_exists(ADDR);

        create_local_executer_for_block_on(async {
            let wg = LocalWaitGroup::new();
            wg.inc();
            let wg_clone = wg.clone();

            local_executor().spawn_local(async move {
                let mut listener = UnixListener::bind(ADDR).await.expect("bind failed");

                wg_clone.done();

                let (mut stream, _) = listener.accept().await.expect("accept failed");

                for _ in 0..TIMES {
                    stream.poll_recv().await.expect("poll failed");
                    let mut buf = vec![0u8; REQUEST.len()];

                    stream.peek_exact(&mut buf).await.expect("peek failed");
                    assert_eq!(REQUEST, buf);
                    stream.peek_exact(&mut buf).await.expect("peek failed");
                    assert_eq!(REQUEST, buf);

                    stream.recv_exact(&mut buf).await.expect("recv failed");
                    assert_eq!(REQUEST, buf);

                    stream.send_all(RESPONSE).await.expect("send failed");
                }
            });

            wg.wait().await;

            let mut stream = UnixStream::connect_with_timeout(ADDR, Duration::from_secs(2))
                .await
                .expect("connect with timeout failed");

            for _ in 0..TIMES {
                stream.poll_send().await.expect("poll failed");
                stream
                    .send_all_with_timeout(REQUEST, Duration::from_secs(2))
                    .await
                    .expect("send with timeout failed");

                stream
                    .poll_recv_with_timeout(Duration::from_secs(2))
                    .await
                    .expect("poll with timeout failed");
                let mut buf = vec![0u8; RESPONSE.len()];
                stream
                    .peek_with_timeout(&mut buf, Duration::from_secs(2))
                    .await
                    .expect("peek with timeout failed");
                stream
                    .peek_with_timeout(&mut buf, Duration::from_secs(2))
                    .await
                    .expect("peek with timeout failed");
                stream
                    .recv_with_timeout(&mut buf, Duration::from_secs(2))
                    .await
                    .expect("recv with timeout failed");
                assert_eq!(RESPONSE, buf);
            }
        });
    }
}
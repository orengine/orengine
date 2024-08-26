//! This module contains [`TcpStream`].
use std::io::Result;
use std::mem;
use socket2::{Domain, Type};

use crate::io::shutdown::AsyncShutdown;
use crate::io::sys::{AsFd, AsRawFd, RawFd, IntoRawFd, FromRawFd, BorrowedFd};
use crate::io::{AsyncClose, AsyncConnectStream, AsyncPeek, AsyncPollFd, AsyncRecv, AsyncSend};
use crate::net::Stream;
use crate::runtime::local_executor;

/// A TCP stream between a local and a remote socket.
///
/// # Close
///
/// [`TcpStream`] is automatically closed after it is dropped.
pub struct TcpStream {
    fd: RawFd,
}

impl AsRawFd for TcpStream {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsFd for TcpStream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}

impl Into<std::net::TcpStream> for TcpStream {
    fn into(self) -> std::net::TcpStream {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::net::TcpStream::from_raw_fd(fd) }
    }
}

impl From<std::net::TcpStream> for TcpStream {
    fn from(stream: std::net::TcpStream) -> Self {
        Self {
            fd: stream.into_raw_fd(),
        }
    }
}

impl IntoRawFd for TcpStream {
    #[inline(always)]
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsyncConnectStream for TcpStream {
    async fn new_ip4() -> Result<Self> {
        Ok(Self {
            fd: crate::io::Socket::new(Domain::IPV4, Type::STREAM).await?
        })
    }

    async fn new_ip6() -> Result<Self> {
        Ok(Self {
            fd: crate::io::Socket::new(Domain::IPV6, Type::STREAM).await?
        })
    }
}

impl AsyncPollFd for TcpStream {}

impl AsyncSend for TcpStream {}

impl AsyncRecv for TcpStream {}

impl AsyncPeek for TcpStream {}

impl AsyncShutdown for TcpStream {}

impl AsyncClose for TcpStream {}

impl crate::net::Socket for TcpStream {}

impl Stream for TcpStream {}

impl Drop for TcpStream {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_future(async {
            close_future.await.expect("Failed to close TCP stream");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::{io, thread};
    use std::rc::Rc;
    use std::time::{Duration, Instant};
    use crate::io::{AsyncAccept, AsyncBind};

    use crate::io::bind::BindConfig;
    use crate::net::Socket;
    use crate::net::tcp::TcpListener;
    use crate::runtime::{create_local_executer_for_block_on, local_executor};
    use crate::sync::cond_var::LocalCondVar;
    use crate::sync::{LocalMutex, LocalWaitGroup};

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[test_macro::test]
    fn test_client() {
        const ADDR: &str = "127.0.0.1:6086";

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            use std::io::{Read, Write};
            let listener = std::net::TcpListener::bind(ADDR).expect("std bind failed");

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

        let (is_server_ready_mu, condvar) = &*is_server_ready_server_clone;
        let mut is_server_ready = is_server_ready_mu.lock().unwrap();
        while *is_server_ready == false {
            is_server_ready = condvar.wait(is_server_ready).unwrap();
        }

        let mut stream = TcpStream::connect(ADDR).await.expect("connect failed");

        for _ in 0..TIMES {
            stream.send_all(REQUEST).await.expect("send failed");

            stream.poll_recv().await.expect("poll failed");
            let mut buf = vec![0u8; RESPONSE.len()];
            stream.recv_exact(&mut buf).await.expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    #[test_macro::test]
    fn test_server() {
        const ADDR: &str = "127.0.0.1:6081";

        let is_server_ready = Arc::new(AtomicBool::new(false));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            create_local_executer_for_block_on(async move {
                let mut listener = TcpListener::bind(ADDR).await.expect("bind failed");

                is_server_ready_server_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                let mut stream = listener.accept().await.expect("accept failed").0;

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

        let mut stream = std::net::TcpStream::connect(ADDR).expect("connect failed");

        for _ in 0..TIMES {
            stream.write_all(REQUEST).expect("send failed");

            let mut buf = vec![0u8; RESPONSE.len()];
            stream.read_exact(&mut buf).expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    #[test_macro::test]
    fn test_stream() {
        const ADDR: &str = "127.0.0.1:6082";

        let wg = Rc::new(LocalWaitGroup::new());
        wg.inc();
        let wg_clone = wg.clone();

        local_executor().spawn_local(async move {
            let mut listener = TcpListener::bind(ADDR).await.expect("bind failed");

            wg_clone.done();

            let mut stream = listener.accept().await.expect("accept failed").0;

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

        let mut stream = TcpStream::connect_with_timeout(ADDR, Duration::from_secs(2))
            .await
            .expect("connect with timeout failed");

        stream.set_ttl(133).expect("set_ttl failed");
        assert_eq!(stream.ttl().expect("get_ttl failed"), 133);

        stream.set_nodelay(true).expect("set_nodelay failed");
        assert_eq!(stream.nodelay().expect("get_nodelay failed"), true);
        stream.set_nodelay(false).expect("set_nodelay failed");
        assert_eq!(stream.nodelay().expect("get_nodelay failed"), false);

        stream
            .set_linger(Some(Duration::from_secs(23)))
            .expect("set_linger failed");
        assert_eq!(
            stream.linger().expect("get_linger failed"),
            Some(Duration::from_secs(23))
        );

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
    }

    #[test_macro::test]
    fn test_timeout() {
        const ADDR: &str = "127.0.0.1:6083";
        const BACKLOG_SIZE: isize = 256;

        const CONNECT: usize = 0;
        const SEND: usize = 1;
        const POLL: usize = 2;
        const RECV: usize = 3;
        const PEEK: usize = 4;
        const TIMEOUT: Duration = Duration::from_millis(1);

        let state = Rc::new(LocalMutex::new(CONNECT));
        let state_cond_var = Rc::new(LocalCondVar::new());
        let state_clone = state.clone();
        let state_cond_var_clone = state_cond_var.clone();
        let wg = Rc::new(LocalWaitGroup::new());
        wg.inc();
        let wg_clone = wg.clone();

        local_executor().spawn_local(async move {
            let mut listener =
                TcpListener::bind_with_config(ADDR, &BindConfig::new().backlog_size(BACKLOG_SIZE))
                    .await
                    .expect("bind failed");
            let mut expected_state = 0;
            let mut state = state_clone.lock().await;

            wg_clone.done();

            loop {
                while *state != expected_state {
                    state = state_cond_var_clone.wait(state).await;
                }
                match *state {
                    CONNECT => {}
                    SEND => {
                        let _ = listener.accept().await.expect("accept failed").0;
                    }
                    POLL | PEEK | RECV => {
                        let _ = listener.accept().await.expect("accept failed").0;
                    }
                    _ => break,
                }
                expected_state += 1;
            }
        });

        wg.wait().await;

        loop {
            let mut state = state.lock().await;
            match *state {
                CONNECT => {
                    for _ in 0..BACKLOG_SIZE + 1 {
                        let _ = TcpStream::connect_with_timeout(ADDR, TIMEOUT)
                            .await
                            .expect("connect with timeout failed");
                    }
                    let res = TcpStream::connect_with_timeout(ADDR, TIMEOUT).await;
                    match res {
                        Ok(_) => panic!("connect with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!("connect with timeout should failed with TimedOut, but got {:?}", err)
                        }
                        Err(_) => {}
                    }
                }

                SEND => {
                    let mut stream = TcpStream::connect_with_timeout(ADDR, TIMEOUT)
                        .await
                        .expect("connect with timeout failed");

                    let buf = vec![0u8; 1 << 24]; // 1 MB.
                                                  // It is impossible to send 1 MB in 1 microsecond (1 TB/s).
                    let res = stream
                        .send_all_with_deadline(&buf, Instant::now() + Duration::from_micros(1))
                        .await;
                    match res {
                        Ok(_) => panic!("send with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!(
                                "send with timeout should failed with TimedOut, but got {:?}",
                                err
                            )
                        }
                        Err(_) => {}
                    }
                }

                POLL => {
                    let stream = TcpStream::connect_with_timeout(ADDR, TIMEOUT)
                        .await
                        .expect("connect with timeout failed");

                    let res = stream.poll_recv_with_timeout(TIMEOUT).await;
                    match res {
                        Ok(_) => panic!("poll with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!(
                                "poll with timeout should failed with TimedOut, but got {:?}",
                                err
                            )
                        }
                        Err(_) => {}
                    }
                }

                RECV => {
                    let mut stream = TcpStream::connect_with_timeout(ADDR, TIMEOUT)
                        .await
                        .expect("connect with timeout failed");

                    let mut buf = vec![0u8; REQUEST.len()];
                    let res = stream.recv_with_timeout(&mut buf, TIMEOUT).await;
                    match res {
                        Ok(_) => panic!("recv with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!(
                                "recv with timeout should failed with TimedOut, but got {:?}",
                                err
                            )
                        }
                        Err(_) => {}
                    }
                }

                PEEK => {
                    let mut stream = TcpStream::connect_with_timeout(ADDR, TIMEOUT)
                        .await
                        .expect("connect with timeout failed");

                    let mut buf = vec![0u8; REQUEST.len()];
                    let res = stream.peek_with_timeout(&mut buf, TIMEOUT).await;
                    match res {
                        Ok(_) => panic!("peek with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!(
                                "peek with timeout should failed with TimedOut, but got {:?}",
                                err
                            )
                        }
                        Err(_) => {}
                    }
                }

                _ => break,
            }
            *state += 1;
            state_cond_var.notify_one();
        }
    }
}

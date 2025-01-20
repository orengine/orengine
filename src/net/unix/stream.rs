//! This module contains [`UnixStream`].

use crate::io::shutdown::AsyncShutdown;
use crate::io::sys::{AsRawSocket, AsSocket, FromRawSocket, IntoRawSocket, RawSocket};
use crate::io::{
    AsyncConnectStream, AsyncPeek, AsyncPollSocket, AsyncRecv, AsyncSend, AsyncSocketClose,
};
use crate::net::creators_of_sockets::new_unix_stream;
use crate::net::unix::unix_impl_socket;
use crate::net::{Socket, Stream};
use crate::runtime::local_executor;
use std::fmt::{Debug, Formatter};
use std::io::Result;
use std::mem::ManuallyDrop;

/// A Unix stream socket.
///
/// # OS Support
///
/// This structure is only supported on Unix platforms and is not available on Windows.
///
/// # Close
///
/// [`UnixStream`] is automatically closed after it is dropped.
///
/// # Example
///
/// ```rust
/// use orengine::io::{full_buffer, AsyncAccept, AsyncBind};
/// use orengine::local_executor;
/// use orengine::net::{Stream, UnixListener};
///
/// async fn handle_stream<S: Stream>(mut stream: S) {
///     loop {
///         stream.poll_recv().await.expect("poll_recv was failed");
///         let mut buf = full_buffer();
///         let n = stream.recv(&mut buf).await.expect("recv was failed");
///         if n == 0 {
///             break;
///         }
///         
///         buf.clear();
///         buf.append(b"pong");
///
///         stream.send_all(&buf).await.expect("send_all was failed");
///     }
/// }
///
/// async fn run_server() -> std::io::Result<()> {
///     let mut listener = UnixListener::bind("/tmp/test").await?;
///     while let Ok((stream, addr)) = listener.accept().await {
///         local_executor().spawn_local(async move {
///             handle_stream(stream).await;
///         });
///     }
///     Ok(())
/// }
/// ```
pub struct UnixStream {
    raw_socket: RawSocket,
}

impl std::os::fd::IntoRawFd for UnixStream {
    fn into_raw_fd(self) -> std::os::fd::RawFd {
        ManuallyDrop::new(self).raw_socket
    }
}

impl IntoRawSocket for UnixStream {}

impl std::os::fd::AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.raw_socket
    }
}

impl AsRawSocket for UnixStream {}

impl std::os::fd::AsFd for UnixStream {
    fn as_fd(&self) -> std::os::fd::BorrowedFd {
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.raw_socket) }
    }
}

impl AsSocket for UnixStream {}

impl std::os::fd::FromRawFd for UnixStream {
    unsafe fn from_raw_fd(raw_fd: std::os::fd::RawFd) -> Self {
        Self { raw_socket: raw_fd }
    }
}

impl FromRawSocket for UnixStream {}

impl From<UnixStream> for std::os::unix::net::UnixStream {
    fn from(stream: UnixStream) -> Self {
        unsafe { Self::from_raw_socket(ManuallyDrop::new(stream).raw_socket) }
    }
}

impl From<std::os::unix::net::UnixStream> for UnixStream {
    fn from(stream: std::os::unix::net::UnixStream) -> Self {
        Self {
            raw_socket: IntoRawSocket::into_raw_socket(stream),
        }
    }
}

impl AsyncPollSocket for UnixStream {}

impl Socket for UnixStream {
    unix_impl_socket!();
}

impl AsyncConnectStream for UnixStream {
    async fn new_for_addr(_: &Self::Addr) -> Result<Self> {
        Ok(Self {
            raw_socket: new_unix_stream().await?,
        })
    }
}

impl AsyncSend for UnixStream {}

impl AsyncRecv for UnixStream {}

impl AsyncPeek for UnixStream {}

impl AsyncShutdown for UnixStream {}

impl AsyncSocketClose for UnixStream {}

impl Stream for UnixStream {}

impl Debug for UnixStream {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("UnixStream");

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

impl Drop for UnixStream {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_local_future(async {
            close_future.await.expect("Failed to close UNIX stream");
        });
    }
}

#[cfg(test)]
mod tests {
    use crate as orengine;
    use crate::io::{
        buffer, get_fixed_buffer, AsyncAccept, AsyncBind, AsyncConnectStream, AsyncPeek,
        AsyncPollSocket, AsyncRecv, AsyncSend, FixedBuffer,
    };
    use crate::net::{BindConfig, UnixListener, UnixStream};
    use crate::sync::{
        AsyncCondVar, AsyncMutex, AsyncWaitGroup, LocalCondVar, LocalMutex, LocalWaitGroup,
    };
    use crate::{fs, local_executor};
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use std::{io, thread};

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\n";
    const TIMES: usize = 20;

    #[orengine::test::test_local]
    fn test_unix_client() {
        const ADDR: &str = "/tmp/orengine_test_unix_client";

        let _ = fs::remove_file(ADDR).await;

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        let server_thread = thread::spawn(move || {
            use std::io::{Read, Write};

            let listener = std::os::unix::net::UnixListener::bind(ADDR).expect("std bind failed");

            {
                let (is_ready_mu, condvar) = &*is_server_ready;
                let mut is_ready = is_ready_mu.lock().expect("lock failed");
                *is_ready = true;
                drop(is_ready);
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

        let mut stream = UnixStream::connect(ADDR).await.expect("connect failed");

        for _ in 0..TIMES {
            stream.send_all_bytes(REQUEST).await.expect("send failed");

            stream.poll_recv().await.expect("poll failed");
            let mut buf = vec![0u8; RESPONSE.len()];
            stream
                .recv_bytes_exact(&mut buf)
                .await
                .expect("recv failed");
            assert_eq!(RESPONSE, buf);
        }

        server_thread.join().expect("server thread join failed");
    }

    #[orengine::test::test_local]
    fn test_unix_server() {
        const ADDR: &str = "/tmp/orengine_test_unix_server";

        let _ = fs::remove_file(ADDR).await;

        let is_server_ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let is_server_ready_server_clone = is_server_ready.clone();

        thread::spawn(move || {
            use std::io::{Read, Write};

            {
                loop {
                    let is_server_ready =
                        is_server_ready_server_clone.0.lock().expect("lock failed");
                    if *is_server_ready {
                        drop(is_server_ready);
                        break;
                    }

                    let _unused = is_server_ready_server_clone
                        .1
                        .wait(is_server_ready)
                        .expect("wait failed");
                }
            }

            let mut stream = std::os::unix::net::UnixStream::connect(ADDR).expect("connect failed");

            for _ in 0..TIMES {
                stream.write_all(REQUEST).expect("send failed");

                let mut buf = vec![0u8; RESPONSE.len()];
                stream.read_exact(&mut buf).expect("recv failed");
                assert_eq!(RESPONSE, buf);
            }
        });

        let mut listener = UnixListener::bind(ADDR).await.expect("bind failed");

        *is_server_ready.0.lock().unwrap() = true;
        is_server_ready.1.notify_all();

        let mut stream = listener.accept().await.expect("accept failed").0;

        for _ in 0..TIMES {
            stream.poll_recv().await.expect("poll failed");
            let mut buf = buffer();
            buf.set_len(u32::try_from(REQUEST.len()).unwrap()).unwrap();
            stream.recv_exact(&mut buf).await.expect("recv failed");
            assert_eq!(REQUEST, buf.as_bytes());

            buf.clear();
            buf.append(RESPONSE);

            stream.send_all(&buf).await.expect("send failed");
        }
    }

    #[orengine::test::test_local]
    fn test_unix_stream() {
        const ADDR: &str = "/tmp/orengine_test_unix_stream";

        let _ = fs::remove_file(ADDR).await;

        let mut buffered_request = get_fixed_buffer().await;
        buffered_request.append(REQUEST);
        assert_eq!(REQUEST, buffered_request.as_bytes());

        let wg = Rc::new(LocalWaitGroup::new());
        wg.inc();
        let wg_clone = wg.clone();

        local_executor().spawn_local(async move {
            let mut listener = UnixListener::bind(ADDR).await.expect("bind failed");

            wg_clone.done();

            let mut stream = listener.accept().await.expect("accept failed").0;

            for _ in 0..TIMES {
                stream.poll_recv().await.expect("poll failed");
                let mut buf = vec![0u8; REQUEST.len()];

                stream
                    .peek_bytes_exact(&mut buf)
                    .await
                    .expect("peek failed");
                assert_eq!(REQUEST, buf);
                stream
                    .peek_bytes_exact(&mut buf)
                    .await
                    .expect("peek failed");
                assert_eq!(REQUEST, buf);

                stream
                    .recv_bytes_exact(&mut buf)
                    .await
                    .expect("recv failed");
                assert_eq!(REQUEST, buf);

                stream.send_all_bytes(RESPONSE).await.expect("send failed");
            }
        });

        wg.wait().await;

        let mut stream = UnixStream::connect_with_timeout(ADDR, Duration::from_secs(2))
            .await
            .expect("connect with timeout failed");

        for _ in 0..TIMES {
            buffered_request.clear();
            buffered_request.append(REQUEST);
            stream.poll_send().await.expect("poll failed");
            stream
                .send_all_with_timeout(&buffered_request, Duration::from_secs(2))
                .await
                .expect("send with timeout failed");

            stream
                .poll_recv_with_timeout(Duration::from_secs(2))
                .await
                .expect("poll with timeout failed");
            buffered_request
                .set_len(u32::try_from(RESPONSE.len()).unwrap())
                .unwrap();
            stream
                .peek_exact_with_timeout(&mut buffered_request, Duration::from_secs(2))
                .await
                .expect("peek with timeout failed");
            assert_eq!(RESPONSE, buffered_request.as_bytes());

            stream
                .peek_exact_with_timeout(&mut buffered_request, Duration::from_secs(2))
                .await
                .expect("peek with timeout failed");
            assert_eq!(RESPONSE, buffered_request.as_bytes());

            stream
                .recv_with_timeout(&mut buffered_request, Duration::from_secs(2))
                .await
                .expect("recv with timeout failed");
            assert_eq!(RESPONSE, buffered_request.as_bytes());
        }
    }

    #[orengine::test::test_local]
    fn test_unix_timeout() {
        const ADDR: &str = "/tmp/orengine_test_unix_timeout";

        let _ = fs::remove_file(ADDR).await;

        const SEND: usize = 0;
        const POLL: usize = 1;
        const RECV: usize = 2;
        const PEEK: usize = 3;
        const TIMEOUT: Duration = Duration::from_millis(100);

        let state = Rc::new(LocalMutex::new(SEND));
        let state_cond_var = Rc::new(LocalCondVar::new());
        let state_clone = state.clone();
        let state_cond_var_clone = state_cond_var.clone();
        let wg = Rc::new(LocalWaitGroup::new());
        wg.inc();
        let wg_clone = wg.clone();

        local_executor().spawn_local(async move {
            let mut listener = UnixListener::bind_with_config(ADDR, &BindConfig::new())
                .await
                .expect("bind failed");
            let mut expected_state = 0;

            wg_clone.done();

            loop {
                let mut guard = state_clone.lock().await;
                while *guard != expected_state {
                    guard = state_cond_var_clone.wait(guard).await;
                }

                drop(guard);

                match expected_state {
                    SEND => {
                        let stream = listener.accept().await.expect("accept failed").0;
                        let _ = stream.poll_send().await;
                    }
                    RECV | PEEK | POLL => {
                        let stream = listener.accept().await.expect("accept failed").0;
                        let _ = stream.poll_recv().await;
                    }
                    _ => break,
                }
                expected_state += 1;
            }
        });

        wg.wait().await;

        loop {
            let current_state = *state.lock().await;

            match current_state {
                SEND => {
                    let mut stream = UnixStream::connect_with_timeout(ADDR, TIMEOUT)
                        .await
                        .expect("connect with timeout failed");

                    let buf = vec![0u8; 1 << 24];
                    let res = stream
                        .send_all_bytes_with_deadline(
                            &buf,
                            Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
                        )
                        .await;
                    match res {
                        Ok(()) => panic!("send with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!("send with timeout should failed with TimedOut, but got {err:?}")
                        }
                        Err(_) => {}
                    }
                }

                POLL => {
                    let stream = UnixStream::connect_with_timeout(ADDR, TIMEOUT)
                        .await
                        .expect("connect with timeout failed");

                    let res = stream.poll_recv_with_timeout(TIMEOUT).await;
                    match res {
                        Ok(()) => panic!("poll with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!("poll with timeout should failed with TimedOut, but got {err:?}")
                        }
                        Err(_) => {}
                    }
                }

                RECV => {
                    let mut stream = UnixStream::connect_with_timeout(ADDR, TIMEOUT)
                        .await
                        .expect("connect with timeout failed");

                    let mut buf = vec![0u8; REQUEST.len()];
                    let res = stream.recv_bytes_with_timeout(&mut buf, TIMEOUT).await;
                    match res {
                        Ok(_) => panic!("recv with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!("recv with timeout should failed with TimedOut, but got {err:?}")
                        }
                        Err(_) => {}
                    }
                }

                PEEK => {
                    let mut stream = UnixStream::connect_with_timeout(ADDR, TIMEOUT)
                        .await
                        .expect("connect with timeout failed");

                    let mut buf = vec![0u8; REQUEST.len()];
                    let res = stream.peek_bytes_with_timeout(&mut buf, TIMEOUT).await;
                    match res {
                        Ok(_) => panic!("peek with timeout should failed"),
                        Err(err) if err.kind() != io::ErrorKind::TimedOut => {
                            panic!("peek with timeout should failed with TimedOut, but got {err:?}")
                        }
                        Err(_) => {}
                    }
                }

                _ => break,
            }
            *state.lock().await += 1;
            state_cond_var.notify_one();
        }
    }
}

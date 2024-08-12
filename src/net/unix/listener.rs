use std::{io, mem};
use socket2::{SockAddr, SockRef};

use crate::io::sys::{AsRawFd, RawFd, IntoRawFd, FromRawFd, AsFd, BorrowedFd};
use crate::io::{AsyncClose, AsyncAcceptUnix, AsyncBindUnix, AsPath};
use crate::net::creators_of_sockets::new_unix_socket;
use crate::net::unix::path_listener::PathListener;
use crate::net::UnixStream;
use crate::runtime::local_executor;

pub struct UnixListener {
    pub(crate) fd: RawFd,
}

impl Into<std::os::unix::net::UnixListener> for UnixListener {
    fn into(self) -> std::os::unix::net::UnixListener {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) }
    }
}

impl From<std::os::unix::net::UnixListener> for UnixListener {
    fn from(listener: std::os::unix::net::UnixListener) -> Self {
        Self {
            fd: listener.into_raw_fd(),
        }
    }
}

impl FromRawFd for UnixListener {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl AsRawFd for UnixListener {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsFd for UnixListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) }
    }
}

impl AsyncClose for UnixListener {}

impl AsyncAcceptUnix<UnixStream> for UnixListener {}

impl AsyncBindUnix for UnixListener {
    #[inline(always)]
    async fn bind_with_backlog_size<P: AsPath>(path: P, backlog_size: isize) -> io::Result<Self> {
        let fd = new_unix_socket().await?;
        let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
        let socket_ref = SockRef::from(&borrowed_fd);

        socket_ref.bind(&SockAddr::unix(path)?)?;
        socket_ref.listen(backlog_size as i32)?;

        Ok(Self {
            fd
        })
    }
}

impl PathListener<UnixStream> for UnixListener {}

impl Drop for UnixListener {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future.await.expect("Failed to close unix listener");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use crate::fs::test_helper::delete_file_if_exists;
    use crate::io::AsyncConnectStreamUnix;
    use crate::net::unix::path_socket::PathSocket;
    use crate::net::unix::path_stream::PathStream;
    use crate::net::unix::UnixStream;
    use crate::runtime::create_local_executer_for_block_on;

    use super::*;

    #[test]
    fn test_listener() {
        create_local_executer_for_block_on(async {
            let addr = "/tmp/test_unix_listener.sock";

            delete_file_if_exists(addr);

            let listener = UnixListener::bind(addr).await.expect("bind call failed");

            assert_eq!(
                listener.local_addr().unwrap().as_pathname().unwrap(),
                Path::new(addr)
            );

            match listener.take_error() {
                Ok(err) => {
                    assert!(err.is_none());
                }
                Err(_) => panic!("take_error call failed"),
            }
        });
    }

    #[test]
    fn test_accept() {
        create_local_executer_for_block_on(async {
            const ADDR: &str = "/tmp/test_unix_listener_accept.sock";

            delete_file_if_exists(ADDR);

            let mut listener = UnixListener::bind(ADDR).await.expect("bind call failed");

            local_executor().spawn_local(async {
                let stream = UnixStream::connect(ADDR).await.expect("connect failed");
                assert_eq!(
                    stream.peer_addr().unwrap().as_pathname().unwrap(),
                    Path::new(ADDR)
                );
            });

            let (stream, addr) = listener.accept().await.expect("accept failed");
            assert_eq!(
                stream.local_addr().unwrap().as_pathname().unwrap(),
                Path::new(ADDR)
            );
            assert_eq!(
                addr.as_pathname().unwrap(),
                Path::new(ADDR)
            );
        });
    }
}

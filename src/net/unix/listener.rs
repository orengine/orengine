use std::ffi::c_int;
use std::io::Error;
use std::net::{ToSocketAddrs};
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::path::Path;
use std::{io, mem};
use socket2::SockAddr;
use crate::generate_accept;

use crate::io::sys::{AsFd, Fd};
use crate::io::{AsyncClose, Bind};
use crate::net::creators_of_sockets::new_unix_socket;
use crate::net::UnixStream;
use crate::runtime::local_executor;

pub struct Listener {
    pub(crate) fd: Fd,
}

// TODO update docs
impl Listener {
    /// Returns the state_ptr of the [`Listener`].
    ///
    /// Uses for low-level work with the scheduler. If you don't know what it is, don't use it.
    #[inline(always)]
    pub fn fd(&mut self) -> Fd {
        self.fd
    }

    #[inline(always)]
    #[cfg(unix)]
    pub fn borrow_fd(&self) -> BorrowedFd {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }

    #[inline(always)]
    pub fn bind_with_backlog_size<P: AsRef<Path>>(path: P, backlog_size: i32) -> io::Result<Self> {
        let addr = SockAddr::unix(path.as_ref())?;
        let socket = new_unix_socket()?;

        socket.bind(&addr)?;
        socket.listen(backlog_size as c_int)?;

        Ok(Self {
            fd: socket.into_raw_fd(),
        })
    }

    #[inline(always)]
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::bind_with_backlog_size(path, 1)
    }

    generate_accept!(UnixStream);

    pub fn local_addr(&self) -> io::Result<std::os::unix::net::SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_unix().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    /// Gets the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::io::Bind;
    /// use orengine::net::TcpListener;
    ///
    /// let listener = TcpListener::bind("127.0.0.1:80").unwrap();
    /// listener.take_error().expect("No error was expected");
    /// ```
    pub fn take_error(&self) -> io::Result<Option<Error>> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.take_error()
    }
}

impl Into<std::os::unix::net::UnixListener> for Listener {
    fn into(self) -> std::os::unix::net::UnixListener {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) }
    }
}

impl From<std::os::unix::net::UnixListener> for Listener {
    fn from(listener: std::os::unix::net::UnixListener) -> Self {
        Self {
            fd: listener.into_raw_fd(),
        }
    }
}

impl From<Fd> for Listener {
    fn from(fd: Fd) -> Self {
        Self { fd }
    }
}

impl AsFd for Listener {
    #[inline(always)]
    fn as_raw_fd(&self) -> Fd {
        self.fd
    }
}

impl AsyncClose for Listener {}

impl Drop for Listener {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future.await.expect("Failed to close unix listener");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::SocketAddr;
    use crate::fs::test_helper::delete_file_if_exists;
    use crate::net::unix::Stream;
    use crate::runtime::create_local_executer_for_block_on;

    use super::*;

    #[test]
    fn test_listener() {
        create_local_executer_for_block_on(async {
            let addr = "/tmp/test_unix_listener.sock";

            delete_file_if_exists(addr);

            let listener = Listener::bind(addr).expect("bind call failed");

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

            let mut listener = Listener::bind(ADDR).expect("bind call failed");

            local_executor().spawn_local(async {
                let stream = Stream::connect(ADDR).await.expect("connect failed");
                assert_eq!(
                    stream.peer_addr().unwrap().as_pathname().unwrap(),
                    Path::new(ADDR)
                );
            });

            let mut stream = listener.accept().await.expect("accept failed");
            assert_eq!(
                stream.local_addr().unwrap().as_pathname().unwrap(),
                Path::new(ADDR)
            );
        });
    }
}

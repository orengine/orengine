use std::ffi::c_int;
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::os::fd::{BorrowedFd, FromRawFd, IntoRawFd};
use std::path::Path;
use std::{io, mem};

use crate::io::sys::{AsFd, Fd};
use crate::io::{AsyncAccept, AsyncClose, Bind};
use crate::net::creators_of_sockets::new_unix_socket;
use crate::net::unix::bind_config::BindConfig;
use crate::net::unix::Stream;
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
    pub fn bind_with_config<P: AsRef<Path>>(path: P, config: &BindConfig) -> io::Result<Self> {
        let addr = socket2::SockAddr::unix(path.as_ref())?;
        let socket = new_unix_socket()?;

        if config.reuse_address {
            socket.set_reuse_address(true)?;
        }

        if config.reuse_port {
            socket.set_reuse_port(true)?;
        }

        socket.bind(&addr)?;
        socket.listen(config.backlog_size as c_int)?;

        Ok(Self {
            fd: socket.into_raw_fd(),
        })
    }

    #[inline(always)]
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::bind_with_config(path, &BindConfig::default())
    }

    /// Returns the local socket address of this listener.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use orengine::io::Bind;
    /// use orengine::net::TcpListener;
    ///
    /// let listener = TcpListener::bind("127.0.0.1:8080").unwrap();
    /// assert_eq!(listener.local_addr().unwrap(), SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_socket().ok_or(Error::new(
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

impl AsyncAccept<Stream> for Listener {}

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
    use crate::runtime::create_local_executer_for_block_on;

    use super::*;

    #[test]
    fn test_listener() {
        create_local_executer_for_block_on(async {
            let addr = "/test_unix_listener.sock";
            let listener = Listener::bind(addr).expect("bind call failed");

            println!("TODO r local_addr: {:?}", listener.local_addr().unwrap());
            // TODO assert_eq!(listener.local_addr().unwrap(), addr);

            match listener.take_error() {
                Ok(err) => {
                    assert!(err.is_none());
                }
                Err(_) => panic!("take_error call failed"),
            }
        });
    }

    // #[test]
    // TODO
    // fn test_accept() {
    //     create_local_executer_for_block_on(async {
    //         let addr = UnixAddr::local_path("/test_unix_listener.sock");
    //         let mut listener = Listener::bind(addr).expect("bind call failed");
    //
    //         // Test accept
    //         let mut stream = listener.accept().await.expect("accept failed").0;
    //         assert_eq!(stream.local_addr().unwrap(), addr);
    //
    //         // Test closing listener
    //         let close_future = listener.close();
    //         local_executor().spawn_local(async {
    //             close_future.await.expect("Failed to close unix listener");
    //         });
    //     });
    // }
}

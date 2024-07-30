//! This module contains [`Listener`].
use std::ffi::c_int;
use std::net::{SocketAddr, ToSocketAddrs};
#[cfg(unix)]
use std::os::fd::{BorrowedFd, FromRawFd};
use std::io::{Error, Result};
use std::{io, mem};
use socket2::{Protocol, SockAddr, Type};
use crate::{each_addr_sync};
use crate::io::{AsyncAccept, AsyncClose, Bind};
use crate::io::bind::BindConfig;
use crate::net::TcpStream;
use crate::runtime::local_executor;
use crate::io::sys::{Fd, AsFd, IntoFd};
use crate::net::get_socket::get_socket;

/// A TCP socket server, listening for connections.
///
/// # Close
///
/// [`Listener`] is automatically closed after it is dropped.
pub struct Listener {
    pub(crate) fd: Fd
}

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

    /// Returns the local socket address of this listener.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use async_engine::io::Bind;
    /// use async_engine::net::TcpListener;
    ///
    /// let listener = TcpListener::bind("127.0.0.1:8080").unwrap();
    /// assert_eq!(listener.local_addr().unwrap(), SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));
    /// ```
    pub fn local_addr(&self) -> Result<SocketAddr> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.local_addr()?.as_socket().ok_or(Error::new(io::ErrorKind::Other, "failed to get local address"))
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::TcpListener;
    ///
    /// let listener = TcpListener::bind("127.0.0.1:80").unwrap();
    /// listener.set_ttl(100).expect("could not set TTL");
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> Result<()> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.set_ttl(ttl)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`Listener::set_ttl`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use async_engine::io::Bind;
    /// use async_engine::net::TcpListener;
    ///
    /// let listener = TcpListener::bind("127.0.0.1:80").unwrap();
    /// listener.set_ttl(100).expect("could not set TTL");
    /// assert_eq!(listener.ttl().unwrap_or(0), 100);
    /// ```
    pub fn ttl(&self) -> Result<u32> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.ttl()
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
    /// use async_engine::io::Bind;
    /// use async_engine::net::TcpListener;
    ///
    /// let listener = TcpListener::bind("127.0.0.1:80").unwrap();
    /// listener.take_error().expect("No error was expected");
    /// ```
    pub fn take_error(&self) -> Result<Option<Error>> {
        let borrowed_fd = self.borrow_fd();
        let socket_ref = socket2::SockRef::from(&borrowed_fd);
        socket_ref.take_error()
    }
}

impl Bind for Listener {
    fn bind_with_config<A: ToSocketAddrs>(addrs: A, config: BindConfig) -> Result<Self> {
        each_addr_sync!(&addrs, move |addr| {
            let socket = get_socket(addr, Type::STREAM, Some(Protocol::TCP))?;
            if config.only_v6 {
                socket.set_only_v6(true)?;
            }

            if config.reuse_address {
               socket.set_reuse_address(true)?;
            }

            if config.reuse_port {
                socket.set_reuse_port(true)?;
            }

            socket.bind(&SockAddr::from(addr))?;
            socket.listen(config.backlog_size as c_int)?;
            Ok(Self {
                fd: socket.into_raw_fd()
            })
        })
    }
}

impl Into<std::net::TcpListener> for Listener {
    fn into(self) -> std::net::TcpListener {
        let fd = self.fd;
        mem::forget(self);

        unsafe {
            std::net::TcpListener::from_raw_fd(fd)
        }
    }
}

impl From<std::net::TcpListener> for Listener {
    fn from(listener: std::net::TcpListener) -> Self {
        Self {
            fd: listener.into_raw_fd()
        }
    }
}

impl From<Fd> for Listener {
    fn from(fd: Fd) -> Self {
        Self {
            fd
        }
    }
}

impl AsFd for Listener {
    #[inline(always)]
    fn as_raw_fd(&self) -> Fd {
        self.fd
    }
}

impl AsyncAccept<TcpStream> for Listener {}

impl AsyncClose for Listener {}

impl Drop for Listener {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future.await.expect("Failed to close tcp listener");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};
    use std::time::Duration;
    use crate::runtime::create_local_executer_for_block_on;
    use super::*;

    #[test]
    fn test_listener() {
        create_local_executer_for_block_on(async {
            let listener = Listener::bind("127.0.0.1:8080").expect("bind call failed");
            assert_eq!(listener.local_addr().unwrap(), SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));

            listener.set_ttl(122).expect("set_ttl call failed");
            assert_eq!(listener.ttl().expect("ttl call failed"), 122);

            match listener.take_error() {
                Ok(err) => {
                    assert!(err.is_none());
                },
                Err(_) => panic!("take_error call failed"),
            }
        });
    }

    #[test]
    fn test_accept() {
        create_local_executer_for_block_on(async {
            let mut listener = Listener::bind("127.0.0.1:8080").expect("bind call failed");
            match listener.accept_with_timeout(Duration::from_micros(1)).await {
                Ok(_) => panic!("accept_with_timeout call failed"),
                Err(err) => {
                    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
                }
            }

            let stream = std::net::TcpStream::connect("127.0.0.1:8080").expect("connect call failed");
            match listener.accept_with_timeout(Duration::from_secs(1)).await {
                Ok((_, addr)) => {
                    assert_eq!(addr, stream.local_addr().unwrap())
                },
                Err(_) => panic!("accept_with_timeout call failed"),
            }
        });
    }
}
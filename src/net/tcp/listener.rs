//! This module contains [`TcpListener`].
use std::ffi::c_int;
use std::fmt::{Debug, Formatter};
use std::io::{Result};
use std::net::ToSocketAddrs;
use std::mem;
use socket2::SockAddr;

use crate::each_addr;
use crate::io::bind::BindConfig;
use crate::io::sys::{AsRawFd, RawFd, AsFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};
use crate::io::{AsyncAccept, AsyncClose, AsyncBind};
use crate::net::creators_of_sockets::new_tcp_socket;
use crate::net::Listener;
use crate::net::tcp::TcpStream;
use crate::runtime::local_executor;

/// A TCP socket server, listening for connections.
///
/// # Close
///
/// [`TcpListener`] is automatically closed after it is dropped.
pub struct TcpListener {
    pub(crate) fd: RawFd,
}

impl Into<std::net::TcpListener> for TcpListener {
    fn into(self) -> std::net::TcpListener {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::net::TcpListener::from_raw_fd(fd) }
    }
}

impl From<std::net::TcpListener> for TcpListener {
    fn from(listener: std::net::TcpListener) -> Self {
        Self {
            fd: listener.into_raw_fd(),
        }
    }
}

impl IntoRawFd for TcpListener {
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl AsRawFd for TcpListener {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsFd for TcpListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}

impl From<OwnedFd> for TcpListener {
    fn from(fd: OwnedFd) -> Self {
        unsafe { Self::from_raw_fd(fd.into_raw_fd()) }
    }
}

impl Into<OwnedFd> for TcpListener {
    fn into(self) -> OwnedFd {
        unsafe { OwnedFd::from_raw_fd(self.into_raw_fd()) }
    }
}

impl AsyncBind for TcpListener {
    async fn bind_with_config<A: ToSocketAddrs>(addrs: A, config: &BindConfig) -> Result<Self> {
        each_addr!(&addrs, async move |addr| {
            let fd = new_tcp_socket(&addr).await?;
            let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
            let socket_ref = socket2::SockRef::from(&borrowed_fd);

            if config.only_v6 {
                socket_ref.set_only_v6(true)?;
            }

            if config.reuse_address {
                socket_ref.set_reuse_address(true)?;
            }

            if config.reuse_port {
                socket_ref.set_reuse_port(true)?;
            }

            socket_ref.bind(&SockAddr::from(addr))?;
            socket_ref.listen(config.backlog_size as c_int)?;

            Ok(Self {
                fd
            })
        })
    }
}

impl AsyncAccept<TcpStream> for TcpListener {}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsyncClose for TcpListener {}

impl Listener<TcpStream> for TcpListener {}

impl Debug for TcpListener {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("TcpListener");

        if let Ok(addr) = self.local_addr() {
            res.field("local addr", &addr);
        }

        let name = if cfg!(windows) { "socket" } else { "fd" };
        res.field(name, &self.as_raw_fd()).finish()
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_future(async {
            close_future.await.expect("Failed to close tcp listener");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use std::time::Duration;

    use super::*;

    #[orengine_macros::test]
    fn test_listener() {
        let listener = TcpListener::bind("127.0.0.1:8080").await.expect("bind call failed");
        assert_eq!(
            listener.local_addr().unwrap(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080))
        );

        listener.set_ttl(122).expect("set_ttl call failed");
        assert_eq!(listener.ttl().expect("ttl call failed"), 122);

        match listener.take_error() {
            Ok(err) => {
                assert!(err.is_none());
            }
            Err(_) => panic!("take_error call failed"),
        }
    }

    #[orengine_macros::test]
    fn test_accept() {
        let mut listener = TcpListener::bind("127.0.0.1:4063").await.expect("bind call failed");
        match listener.accept_with_timeout(Duration::from_micros(1)).await {
            Ok(_) => panic!("accept_with_timeout call failed"),
            Err(err) => {
                assert_eq!(err.kind(), io::ErrorKind::TimedOut);
            }
        }

        let stream =
            std::net::TcpStream::connect("127.0.0.1:4063").expect("connect call failed");
        match listener.accept_with_timeout(Duration::from_secs(1)).await {
            Ok((_, addr)) => {
                assert_eq!(addr, stream.local_addr().unwrap())
            }
            Err(_) => panic!("accept_with_timeout call failed"),
        }
    }
}

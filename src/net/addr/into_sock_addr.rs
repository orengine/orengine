use crate::net::unix::UnixAddr;
use socket2::SockAddr;
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};

/// `FromSockAddr` allows to get [`socket2::SockAddr`](SockAddr) from self.
///
/// It is already implemented for [`SocketAddr`], [`SocketAddrV4`],
/// [`SocketAddrV6`], [`std::os::unix::net::SocketAddr`], [`UnixAddr`].
pub trait IntoSockAddr {
    fn into_sock_addr(self) -> SockAddr;
}

impl IntoSockAddr for SocketAddrV4 {
    fn into_sock_addr(self) -> SockAddr {
        SockAddr::from(self)
    }
}

impl IntoSockAddr for SocketAddrV6 {
    fn into_sock_addr(self) -> SockAddr {
        SockAddr::from(self)
    }
}

impl IntoSockAddr for SocketAddr {
    fn into_sock_addr(self) -> SockAddr {
        SockAddr::from(self)
    }
}

impl IntoSockAddr for SockAddr {
    fn into_sock_addr(self) -> SockAddr {
        self
    }
}

#[cfg(unix)]
impl IntoSockAddr for std::os::unix::net::SocketAddr {
    fn into_sock_addr(self) -> SockAddr {
        SockAddr::from(UnixAddr::from(self))
    }
}

#[cfg(unix)]
impl IntoSockAddr for UnixAddr {
    fn into_sock_addr(self) -> SockAddr {
        SockAddr::from(self)
    }
}

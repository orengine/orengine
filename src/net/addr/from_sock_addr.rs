use crate::net::unix::UnixAddr;

/// `FromSockAddr` can be used to convert [`socket2::SockAddr`] to another type.
///
/// It is already implemented for [`std::net::SocketAddr`], [`std::net::SocketAddrV4`],
/// [`std::net::SocketAddrV6`], [`std::os::unix::net::SocketAddr`], [`UnixAddr`].
pub trait FromSockAddr: Sized {
    /// Convert `socket2::SockAddr` to `Self`.
    fn from_sock_addr(addr: socket2::SockAddr) -> Option<Self>;
}

impl FromSockAddr for std::net::SocketAddr {
    fn from_sock_addr(addr: socket2::SockAddr) -> Option<Self> {
        addr.as_socket()
    }
}

impl FromSockAddr for std::net::SocketAddrV4 {
    fn from_sock_addr(addr: socket2::SockAddr) -> Option<Self> {
        addr.as_socket_ipv4()
    }
}

impl FromSockAddr for std::net::SocketAddrV6 {
    fn from_sock_addr(addr: socket2::SockAddr) -> Option<Self> {
        addr.as_socket_ipv6()
    }
}

impl FromSockAddr for socket2::SockAddr {
    fn from_sock_addr(addr: socket2::SockAddr) -> Option<Self> {
        Some(addr)
    }
}

#[cfg(unix)]
impl FromSockAddr for std::os::unix::net::SocketAddr {
    fn from_sock_addr(addr: socket2::SockAddr) -> Option<Self> {
        Some(Self::from(UnixAddr::from(addr)))
    }
}

#[cfg(unix)]
impl FromSockAddr for UnixAddr {
    fn from_sock_addr(addr: socket2::SockAddr) -> Option<Self> {
        Some(Self::from(addr))
    }
}

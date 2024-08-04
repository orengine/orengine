use std::net::SocketAddr;

use socket2::{Domain, Protocol, Socket, Type};

#[inline(always)]
pub(crate) fn new_socket(
    addr: &SocketAddr,
    socket_type: Type,
    protocol: Option<Protocol>,
) -> std::io::Result<Socket> {
    match addr {
        SocketAddr::V4(_) => Socket::new(Domain::IPV4, socket_type, protocol),
        SocketAddr::V6(_) => Socket::new(Domain::IPV6, socket_type, protocol),
    }
}

#[inline(always)]
pub(crate) fn new_tcp_socket(addr: &SocketAddr) -> std::io::Result<Socket> {
    new_socket(addr, Type::STREAM, None)
}

#[inline(always)]
pub(crate) fn new_udp_socket(addr: &SocketAddr) -> std::io::Result<Socket> {
    new_socket(addr, Type::DGRAM, None)
}

#[inline(always)]
pub(crate) fn new_unix_socket() -> std::io::Result<Socket> {
    Socket::new(Domain::UNIX, Type::STREAM, None)
}

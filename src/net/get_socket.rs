use std::net::SocketAddr;
use socket2::{Domain, Protocol, Socket, Type};

#[inline(always)]
pub(super) fn get_socket(addr: SocketAddr, socket_type: Type, protocol: Option<Protocol>) -> std::io::Result<Socket> {
    match addr {
        SocketAddr::V4(_) => Socket::new(Domain::IPV4, socket_type, protocol),
        SocketAddr::V6(_) => Socket::new(Domain::IPV6, socket_type, protocol),
    }
}
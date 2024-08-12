use std::net::SocketAddr;
use socket2::{Domain, Type};

use crate::io::Socket;
use crate::io::sys::RawFd;

#[inline(always)]
pub async fn new_socket(
    addr: &SocketAddr,
    socket_type: Type
) -> std::io::Result<RawFd> {
    match addr {
        SocketAddr::V4(_) => {
            Socket::new(Domain::IPV4, socket_type).await
        }

        SocketAddr::V6(_) => {
            Socket::new(Domain::IPV6, socket_type).await
        }
    }
}

#[inline(always)]
pub async fn new_tcp_socket(addr: &SocketAddr) -> std::io::Result<RawFd> {
    new_socket(addr, Type::STREAM).await
}

#[inline(always)]
pub async fn new_udp_socket(addr: &SocketAddr) -> std::io::Result<RawFd> {
    new_socket(addr, Type::DGRAM).await
}

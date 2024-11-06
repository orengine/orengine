use std::net::SocketAddr;
use std::os::fd::IntoRawFd;
use socket2::{Domain, Type};

use crate::io::sys::RawFd;

/// Creates a new socket based on the given `SocketAddr` and `Type` (either stream or datagram).
/// This function determines the appropriate domain (IPv4 or IPv6) based on the provided address,
/// and creates a socket accordingly.
///
/// This is an asynchronous function that returns a raw file descriptor representing the new socket.
#[inline(always)]
pub(crate) async fn new_socket(
    addr: &SocketAddr,
    socket_type: Type
) -> std::io::Result<RawFd> {
    match addr {
        SocketAddr::V4(_) => {
            socket2::Socket::new(Domain::IPV4, socket_type, None).map(|s| s.into_raw_fd())
            // TODO Socket::new(Domain::IPV4, socket_type).await
        }

        SocketAddr::V6(_) => {
            socket2::Socket::new(Domain::IPV6, socket_type, None).map(|s| s.into_raw_fd())
            // TODO Socket::new(Domain::IPV6, socket_type).await
        }
    }
}

/// Creates a new TCP socket based on the provided `SocketAddr`.
/// This is a convenience wrapper around [`new_socket`]
/// that sets the socket type to `Type::STREAM` (TCP).
///
/// This function determines whether to create an IPv4 or IPv6 socket based on the address type.
#[inline(always)]
pub(crate) async fn new_tcp_socket(addr: &SocketAddr) -> std::io::Result<RawFd> {
    new_socket(addr, Type::STREAM).await
}

/// Creates a new UDP socket based on the provided `SocketAddr`.
/// This is a convenience wrapper around [`new_socket`]
/// that sets the socket type to `Type::DGRAM` (UDP).
///
/// This function determines whether to create an IPv4 or IPv6 socket based on the address type.
#[inline(always)]
pub(crate) async fn new_udp_socket(addr: &SocketAddr) -> std::io::Result<RawFd> {
    new_socket(addr, Type::DGRAM).await
}

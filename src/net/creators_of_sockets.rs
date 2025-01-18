use crate::io::sys::RawSocket;
use crate::io::Socket;
use socket2::{Domain, Protocol, Type};
use std::net::SocketAddr;

/// Creates a new socket based on the given `SocketAddr` and `Type` (either stream or datagram).
/// This function determines the appropriate domain (IPv4 or IPv6) based on the provided address,
/// and creates a socket accordingly.
///
/// This is an asynchronous function that returns a raw file descriptor representing the new socket.
#[inline(always)]
pub(crate) async fn new_socket(
    addr: &SocketAddr,
    socket_type: Type,
    protocol: Protocol,
) -> std::io::Result<RawSocket> {
    match addr {
        SocketAddr::V4(_) => Socket::new(Domain::IPV4, socket_type, protocol).await,

        SocketAddr::V6(_) => Socket::new(Domain::IPV6, socket_type, protocol).await,
    }
}

/// Creates a new TCP socket based on the provided `SocketAddr`.
/// This is a convenience wrapper around [`new_socket`]
/// that sets the socket type to [`Type::STREAM`] ([`TCP`](Protocol::TCP)).
///
/// This function determines whether to create an IPv4 or IPv6 socket based on the address type.
#[inline(always)]
pub(crate) async fn new_tcp_socket(addr: &SocketAddr) -> std::io::Result<RawSocket> {
    new_socket(addr, Type::STREAM, Protocol::TCP).await
}

/// Creates a new UDP socket based on the provided `SocketAddr`.
/// This is a convenience wrapper around [`new_socket`]
/// that sets the socket type to [`Type::DGRAM`] ([`UDP`](Protocol::UDP)).
///
/// This function determines whether to create an IPv4 or IPv6 socket based on the address type.
#[inline(always)]
pub(crate) async fn new_udp_socket(addr: &SocketAddr) -> std::io::Result<RawSocket> {
    new_socket(addr, Type::DGRAM, Protocol::UDP).await
}

/// Creates a new UNIX socket with [`stream`](Type::STREAM) type.
#[cfg(unix)]
pub async fn new_unix_stream() -> std::io::Result<RawSocket> {
    Socket::new(Domain::UNIX, Type::STREAM, Protocol::from(0)).await
}

/// Creates a new UNIX socket with [`datagram`](Type::DGRAM) type.
#[cfg(unix)]
pub async fn new_unix_datagram() -> std::io::Result<RawSocket> {
    Socket::new(Domain::UNIX, Type::DGRAM, Protocol::from(0)).await
}

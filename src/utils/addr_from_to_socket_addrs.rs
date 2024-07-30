use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};

#[inline(always)]
pub(crate) fn addr_from_to_socket_addrs<A: ToSocketAddrs>(to_addr: A) -> std::io::Result<SocketAddr> {
    Ok(to_addr
        .to_socket_addrs()?
        .next().ok_or(Error::from(ErrorKind::InvalidInput))?)
}
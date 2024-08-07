use std::io::{Error, ErrorKind};
use std::net::{ToSocketAddrs};
use socket2::SockAddr;

#[inline(always)]
pub(crate) fn addr_from_to_sock_addrs<A: ToSocketAddrs>(to_addr: A) -> std::io::Result<SockAddr> {
    Ok(
        SockAddr::from(
            to_addr
                .to_socket_addrs()?
                .next()
                .ok_or(Error::from(ErrorKind::InvalidInput))?
        )
    )
}
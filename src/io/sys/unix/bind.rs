//! TODO r

use std::net::{SocketAddr, ToSocketAddrs};
use std::os::fd::{AsRawFd, OwnedFd};
use nix::sys::socket::{AddressFamily, Backlog, listen, setsockopt, SockType, SockFlag, SockProtocol, bind, SockaddrIn, SockaddrIn6};
use nix::sys::socket::sockopt::{ReuseAddr, ReusePort};

/// The value of `SO_REUSEADDR` and `SO_REUSEPORT`
const OPTVAL: bool = true;

// TODO result and do it via async channel (create macro for it)
/// Returns [`OwnedFd`] for the configured tcp listener.
#[inline]
pub(crate) fn bind_tcp<A: ToSocketAddrs>(socket_addr: A, backlog_size: usize) -> OwnedFd {
    let addr = socket_addr.to_socket_addrs().unwrap().next().unwrap();
    let fd = match addr {
        SocketAddr::V4(addr) => {
            let fd = nix::sys::socket::socket(
                AddressFamily::Inet,
                SockType::Stream,
                SockFlag::SOCK_NONBLOCK,
                SockProtocol::Tcp
            ).expect("cannot create socket");

            setsockopt(&fd, ReuseAddr, &OPTVAL).expect("cannot set SO_REUSEADDR");
            setsockopt(&fd, ReusePort, &OPTVAL).expect("cannot set SO_REUSEPORT");

            bind(fd.as_raw_fd(), &SockaddrIn::from(addr)).expect("cannot bind");
            fd
        }
        SocketAddr::V6(addr) => {
            let fd = nix::sys::socket::socket(
                AddressFamily::Inet6,
                SockType::Stream,
                SockFlag::SOCK_NONBLOCK,
                SockProtocol::Tcp
            ).expect("cannot create socket");

            setsockopt(&fd, ReuseAddr, &OPTVAL).expect("cannot set SO_REUSEADDR");
            setsockopt(&fd, ReusePort, &OPTVAL).expect("cannot set SO_REUSEPORT");

            bind(fd.as_raw_fd(), &SockaddrIn6::from(addr)).expect("cannot bind");
            fd
        }
    };

    listen(&fd, Backlog::new(backlog_size as i32).unwrap()).expect("cannot listen");

    fd
}

// TODO result and do it via async channel (create macro for it)
/// Returns [`OwnedFd`] for the configured udp listener.
#[inline]
pub(crate) fn bind_upd<A: ToSocketAddrs>(socket_addr: A, backlog_size: usize) -> OwnedFd  {
    let addr = socket_addr.to_socket_addrs().unwrap().next().unwrap();
    let fd = match addr {
        SocketAddr::V4(addr) => {
            let fd = nix::sys::socket::socket(
                AddressFamily::Inet,
                SockType::Datagram,
                SockFlag::SOCK_NONBLOCK,
                SockProtocol::Udp
            ).expect("cannot create socket");

            setsockopt(&fd, ReuseAddr, &OPTVAL).expect("cannot set SO_REUSEADDR");
            setsockopt(&fd, ReusePort, &OPTVAL).expect("cannot set SO_REUSEPORT");

            bind(fd.as_raw_fd(), &SockaddrIn::from(addr)).expect("cannot bind");
            fd
        }
        SocketAddr::V6(addr) => {
            let fd = nix::sys::socket::socket(
                AddressFamily::Inet6,
                SockType::Datagram,
                SockFlag::SOCK_NONBLOCK,
                SockProtocol::Udp
            ).expect("cannot create socket");

            setsockopt(&fd, ReuseAddr, &OPTVAL).expect("cannot set SO_REUSEADDR");
            setsockopt(&fd, ReusePort, &OPTVAL).expect("cannot set SO_REUSEPORT");

            bind(fd.as_raw_fd(), &SockaddrIn6::from(addr)).expect("cannot bind");
            fd
        }
    };

    listen(&fd, Backlog::new(backlog_size as i32).unwrap()).expect("cannot listen");

    fd
}
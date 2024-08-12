use std::io;
use std::io::Error;
use std::net::SocketAddr;
use crate::io::sys::{AsFd, AsRawFd, FromRawFd, IntoRawFd};
use crate::io::{AsyncAccept, AsyncClose, AsyncBind};
use crate::net::Stream;

pub trait Listener<S: Stream>:
    AsyncAccept<S> + FromRawFd + AsyncClose + AsyncBind + AsRawFd + AsFd + IntoRawFd {
    #[inline(always)]
    fn local_addr(&self) -> io::Result<SocketAddr> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.local_addr()?.as_socket().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    #[inline(always)]
    fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_ttl(ttl)
    }

    #[inline(always)]
    fn ttl(&self) -> io::Result<u32> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.ttl()
    }

    #[inline(always)]
    fn take_error(&self) -> io::Result<Option<Error>> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.take_error()
    }
}
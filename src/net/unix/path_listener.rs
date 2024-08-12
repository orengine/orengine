use std::io;
use std::io::Error;

use crate::io::sys::{AsFd, AsRawFd, FromRawFd, IntoRawFd};
use crate::io::{AsyncAcceptUnix, AsyncBindUnix, AsyncClose};

pub trait PathListener<S: FromRawFd>:
    AsyncAcceptUnix<S> + FromRawFd + AsyncBindUnix + AsRawFd + AsFd + IntoRawFd + AsyncClose {
    #[inline(always)]
    fn local_addr(&self) -> io::Result<std::os::unix::net::SocketAddr> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.local_addr()?.as_unix().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    #[inline(always)]
    fn take_error(&self) -> io::Result<Option<Error>> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.take_error()
    }
}